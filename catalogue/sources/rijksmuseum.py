"""The Rijksmuseum's open-access paintings.

The museum publishes its collection as Linked Art, one JSON-LD document per
object — but the painting itself, its image and the image's rights statement
sit in three separate documents (object, `VisualItem`, `DigitalObject`), and
the object alone is ~100 KB. Walking that for 4,900 paintings is five requests
an object. The same museum also runs an OAI-PMH endpoint, and its `edm` format
carries everything a row needs — title, maker with birthplace, subject terms,
the public page, the image URL and the rights statement — in one record, fifty
to a page. Set `261208` ("schilderijen") is the paintings, so a full harvest is
under a hundred requests; that is what this module reads.

The only per-object request left is IIIF's `info.json`, for the true pixel
size (the museum's scans run past 14,000 px). Images come from Micrio's IIIF
host and are asked for as `!4000,4000`, which fits within the box without
enlarging, exactly as `geometry.fit_within` predicts. Plain `4000,4000`
without the `!` is not "fit within" here: it answers with a larger file.

Rights are per record in `edm:rights`. Public Domain Mark and CC0 are taken;
`InC` ("In Copyright") appears on a few paintings and is refused. The set
also holds records with no image at all.

Each harvested page is cached as the rows it produced, keyed by its position
in the walk, and each `info.json` is cached by image id, so an interrupted
pass resumes without asking again.
"""

import os
import xml.etree.ElementTree as ET
from typing import Iterator
from urllib.parse import quote

from .. import geometry, http, regions, text

OAI = "https://data.rijksmuseum.nl/oai"
PAINTINGS_SET = "261208"
IIIF = "https://iiif.micr.io"

HOST_GAPS = {"data.rijksmuseum.nl": 1.0, "iiif.micr.io": 1.0}

OPEN_RIGHTS = frozenset(
    {
        "http://creativecommons.org/publicdomain/mark/1.0/",
        "https://creativecommons.org/publicdomain/mark/1.0/",
        "http://creativecommons.org/publicdomain/zero/1.0/",
        "https://creativecommons.org/publicdomain/zero/1.0/",
    }
)
PAINTING_TYPE = "https://id.rijksmuseum.nl/2208"

_NS = {
    "oai": "http://www.openarchives.org/OAI/2.0/",
    "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "dc": "http://purl.org/dc/elements/1.1/",
    "dcterms": "http://purl.org/dc/terms/",
    "edm": "http://www.europeana.eu/schemas/edm/",
    "skos": "http://www.w3.org/2004/02/skos/core#",
    "rdaGr2": "http://rdvocab.info/ElementsGr2/",
}
_ABOUT = "{%s}about" % _NS["rdf"]
_RESOURCE = "{%s}resource" % _NS["rdf"]
_LANG = "{http://www.w3.org/XML/1998/namespace}lang"


def _english(parent: ET.Element, path: str) -> str:
    """The English text of the first `path` child of `parent`, or whatever
    language comes first if none is tagged `en`."""
    found = parent.findall(path, _NS)
    for node in found:
        if node.get(_LANG) == "en" and (node.text or "").strip():
            return text.collapse(node.text)
    for node in found:
        if (node.text or "").strip():
            return text.collapse(node.text)
    return ""


def _labels(root: ET.Element, tag: str) -> dict[str, ET.Element]:
    """The record's contextual entities of one kind (`edm:Agent`, `edm:Place`,
    `skos:Concept`), by the id the object's own fields point at."""
    return {node.get(_ABOUT): node for node in root.iterfind(f".//{tag}", _NS)}


def _title(cho: ET.Element) -> str:
    # `dc:title` carries the short museum title ("The Night Watch...");
    # `dcterms:alternative` is the long descriptive one.
    return _english(cho, "dc:title") or _english(cho, "dcterms:alternative")


def _row_from_record(record: ET.Element) -> dict | None:
    """One OAI record as a half-built row (image id, no size yet), or `None`
    if it is not an open-access painting with a JPEG."""
    root = record.find("oai:metadata/rdf:RDF", _NS)
    if root is None:
        return None  # a deleted record carries a header only
    aggregation = root.find("ore:Aggregation", {"ore": "http://www.openarchives.org/ore/terms/"})
    cho = root.find("edm:ProvidedCHO", _NS)
    if aggregation is None or cho is None:
        return None

    rights = aggregation.find("edm:rights", _NS)
    if rights is None or rights.get(_RESOURCE) not in OPEN_RIGHTS:
        return None
    dc_type = cho.find("dc:type", _NS)
    if dc_type is None or dc_type.get(_RESOURCE) != PAINTING_TYPE:
        return None
    shown_by = aggregation.find("edm:isShownBy", _NS)
    shown_at = aggregation.find("edm:isShownAt", _NS)
    image = (shown_by.get(_RESOURCE) if shown_by is not None else "") or ""
    # https://iiif.micr.io/{imageId}/full/max/0/default.jpg
    if not image.startswith(IIIF + "/"):
        return None
    image_id = image[len(IIIF) + 1 :].split("/")[0]
    object_number = text.collapse(cho.findtext("dc:identifier", default="", namespaces=_NS))
    if not image_id or not object_number:
        return None

    agents = _labels(root, "edm:Agent")
    places = _labels(root, "edm:Place")
    concepts = _labels(root, "skos:Concept")

    makers = [
        _english(agents[c.get(_RESOURCE)], "skos:prefLabel")
        for c in cho.findall("dc:creator", _NS)
        if c.get(_RESOURCE) in agents
    ]
    makers = [m for m in makers if m]
    birthplaces = []
    for c in cho.findall("dc:creator", _NS):
        agent = agents.get(c.get(_RESOURCE))
        born = agent.find("rdaGr2:placeOfBirth", _NS) if agent is not None else None
        place = places.get(born.get(_RESOURCE)) if born is not None else None
        if place is not None:
            birthplaces.append(_english(place, "skos:prefLabel"))
    birthplaces = [b for b in birthplaces if b]

    # Subject terms that are *concepts* ("landscape", "winter"); the sitters
    # and depicted people are `edm:Agent`s and would only add names.
    tags = []
    for s in cho.findall("dc:subject", _NS):
        concept = concepts.get(s.get(_RESOURCE))
        label = _english(concept, "skos:prefLabel") if concept is not None else ""
        if label and label not in tags:
            tags.append(label)

    # The record names the maker's birthplace, never a nationality, and the
    # keyword table only knows countries. The Rijksmuseum is the Netherlands'
    # national gallery, so a place the table does not recognise is Europe
    # rather than a reason to drop the painting — the same default SMK gets.
    region = next(filter(None, map(regions.region_from_text, birthplaces)), None) or "EUROPE"

    return {
        "id": object_number.replace("/", "-").replace(" ", "-"),
        "image_id": image_id,
        "region": region,
        # The record links the Dutch page; the same path under /en/collection/
        # redirects to the English one.
        "details_url": ((shown_at.get(_RESOURCE) if shown_at is not None else "") or "").replace(
            "/nl/collectie/object/", "/en/collection/object/"
        ),
        "title": _title(cho) or "Untitled",
        "byline": text.byline(" and ".join(makers), _english(cho, "dcterms:created")),
        "origin": birthplaces[0] if birthplaces else "",
        "tags": tags,
        "artist": text.maker(makers[0]) if len(makers) == 1 else "",
    }


def _harvest(client: http.PacedClient, cache: http.JsonCache, refresh: bool) -> Iterator[dict]:
    """Every open-access painting in the set, a page at a time. A page's rows
    and the token for the next are cached together under the page's position,
    so the walk resumes from disk until it reaches a page it has not seen."""
    token = None
    page_no = 0
    while True:
        key = f"page-{page_no:04d}"
        page = None if refresh else cache.get(key)
        if page is None:
            if token is None:
                url = f"{OAI}?verb=ListRecords&metadataPrefix=edm&set={PAINTINGS_SET}"
            else:
                url = f"{OAI}?verb=ListRecords&resumptionToken={quote(token, safe='')}"
            root = ET.fromstring(client.get_bytes(url))
            rows = [
                row
                for record in root.iterfind(".//oai:record", _NS)
                if (row := _row_from_record(record)) is not None
            ]
            next_token = root.findtext(".//oai:resumptionToken", default="", namespaces=_NS)
            page = {"rows": rows, "next": next_token.strip(), "examined": len(root.findall(".//oai:record", _NS))}
            cache.put(key, page)
        yield from page["rows"]
        if not page["next"]:
            return
        token = page["next"]
        page_no += 1


def _size(client: http.PacedClient, cache: http.JsonCache, image_id: str, refresh: bool) -> dict:
    """`{"width", "height"}` of the image as `!4000,4000` delivers it, or
    `{"skip": why}`. Only `http.SourceAborted` escapes."""
    record = None if refresh else cache.get(image_id)
    if record is not None:
        return record
    try:
        info = client.get_json(f"{IIIF}/{image_id}/info.json")
        width, height = geometry.fit_within(int(info["width"]), int(info["height"]))
        record = {"width": width, "height": height}
    except http.SourceAborted:
        raise
    except Exception as e:  # noqa: BLE001 — one bad image must not sink the pass
        record = {"skip": f"error: {e}"}
    cache.put(image_id, record)
    return record


def fetch(
    client: http.PacedClient,
    cache_dir: str,
    refresh: bool,
    csv_path: str | None = None,  # unused; part of every source's shared signature
) -> Iterator[dict]:
    pages = http.JsonCache(os.path.join(cache_dir, "oai"))
    sizes = http.JsonCache(os.path.join(cache_dir, "sizes"))
    for row in _harvest(client, pages, refresh):
        size = _size(client, sizes, row["image_id"], refresh)
        if size.get("skip") or not geometry.meets_minimum(size["width"], size["height"]):
            continue
        yield {
            "source": "rijks",
            "id": row["id"],
            "region": row["region"],
            "width": size["width"],
            "height": size["height"],
            "image_url": f"{IIIF}/{row['image_id']}/full/!4000,4000/0/default.jpg",
            "details_url": row["details_url"],
            "title": row["title"],
            "byline": row["byline"],
            "origin": row["origin"],
            "tags": row["tags"],
            "artist": row["artist"],
        }
