"""The J. Paul Getty Museum's open-content paintings.

The museum publishes its collection as Linked Art. A SPARQL query lists every
object typed "Paintings" (AAT 300033618) that has a representation; each is
then one `object/{uuid}` document — title, maker, nationality, date, subject
terms, the public page — and one `media/image/{uuid}` document for the image
the object's `representation` names, which is where the *image's* rights, its
true pixel size and its IIIF service live. Two requests an object, a few
hundred objects.

The rights gate reads the image document, not the object. The object carries
a "License for Collection Metadata" that is CC0 for everything — it licenses
the catalogue record, not the photograph — so trusting it would admit images
the museum has not released. The image's own `Right` must be classed as CC0 or
the Public Domain Mark. (Getty also tags released images with a `download`
clearance level; every CC0 one seen has it, but it is not what is tested.)

Getty's IIIF host answers `!w,h` with 400, so the fit-within request is spelled
out: `max` when the image already fits the box, otherwise `4000,` or `,4000`
along the long side. `geometry.fit_within` predicts what comes back.
"""

import os
import re
from typing import Iterator
from urllib.parse import urlencode

from .. import geometry, http, regions, text

SPARQL = "https://data.getty.edu/museum/collection/sparql"
OBJECT = "https://data.getty.edu/museum/collection/object"
MEDIA = "https://data.getty.edu/media/image"
IIIF = "https://media.getty.edu/iiif/image"

HOST_GAPS = {"data.getty.edu": 1.0, "media.getty.edu": 1.0}

OPEN_RIGHTS = frozenset(
    {
        "http://creativecommons.org/publicdomain/zero/1.0/",
        "http://creativecommons.org/publicdomain/mark/1.0/",
    }
)

LINKED_ART = {"Accept": "application/ld+json"}

PAINTINGS_QUERY = """
PREFIX crm: <http://www.cidoc-crm.org/cidoc-crm/>
PREFIX aat: <http://vocab.getty.edu/aat/>
SELECT DISTINCT ?o WHERE {
  ?o crm:P2_has_type aat:300033618 ;
     crm:P138i_has_representation ?image .
} ORDER BY ?o
"""

_UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")

# What a role statement says when the named person simply made it. Anything
# else ("Attributed to", "Follower of", "Workshop of") is a hand that is only
# suggested, and does not name a painter someone could choose.
_PLAIN_ROLE = "artist"


def _candidates(client: http.PacedClient) -> list[str]:
    url = f"{SPARQL}?{urlencode({'query': PAINTINGS_QUERY})}"
    result = client.get_json(url, headers={"Accept": "application/sparql-results+json"})
    uuids = []
    for binding in result["results"]["bindings"]:
        found = _UUID.search(binding["o"]["value"])
        if found:
            uuids.append(found.group(0))
    return uuids


def _classes(node: dict) -> set[str]:
    return {c.get("id", "") for c in node.get("classified_as") or []}


def _statements(node: dict, class_suffix: str) -> list[str]:
    """Contents of every `referred_to_by` entry under `node`, at any depth,
    whose classification ends in `class_suffix`."""
    found = []
    for ref in node.get("referred_to_by") or []:
        if any(c.endswith(class_suffix) for c in _classes(ref)) and ref.get("content"):
            found.append(text.collapse(ref["content"]))
    for part in node.get("part") or []:
        found.extend(_statements(part, class_suffix))
    return found


def _persons(production: dict) -> list[dict]:
    people = list(production.get("carried_out_by") or [])
    for part in production.get("part") or []:
        people.extend(part.get("carried_out_by") or [])
    return people


def _name_of(obj: dict, field: str) -> str:
    for name in obj.get("identified_by") or []:
        if name.get("type") == "Name" and name.get("_label") == field and name.get("content"):
            return text.collapse(name["content"])
    return ""


def _main_image(obj: dict) -> tuple[str, str] | None:
    """`(media document uuid, IIIF image uuid)` of the object's overall image.
    `representation` names the IIIF image and an asset number; the object's
    `shows` lists its media documents labelled with the same number."""
    for rep in obj.get("representation") or []:
        iiif = _UUID.search(rep.get("id", ""))
        asset = next(
            (i.get("content") for i in rep.get("identified_by") or [] if i.get("_label") == "Asset Name"),
            None,
        )
        if not iiif or not asset:
            continue
        for shown in obj.get("shows") or []:
            if f"Related Media (Image): {asset} /" in (shown.get("_label") or ""):
                media = _UUID.search(shown.get("id", ""))
                if media:
                    return media.group(0), iiif.group(0)
    return None


def _image_facts(media: dict, iiif_uuid: str) -> dict:
    """The image document's rights and pixel size, or `{"skip": why}`."""
    rights = [
        c.get("id")
        for right in media.get("subject_to") or []
        for c in right.get("classified_as") or []
    ]
    if not OPEN_RIGHTS.intersection(rights):
        return {"skip": f"image rights not open: {sorted(r for r in rights if r)[:3]}"}
    for shown in media.get("digitally_shown_by") or []:
        service = [a.get("id", "") for a in shown.get("access_point") or []]
        if f"{IIIF}/{iiif_uuid}" not in service:
            continue
        sizes = {}
        for dim in shown.get("dimension") or []:
            kind = (dim.get("classified_as") or [{}])[0].get("_label")
            if kind in ("Width", "Height") and isinstance(dim.get("value"), (int, float)):
                sizes[kind] = int(dim["value"])
        if len(sizes) == 2:
            return {"width": sizes["Width"], "height": sizes["Height"]}
    return {"skip": "no pixel size for the representation's image"}


def _image_url(iiif_uuid: str, width: int, height: int) -> str:
    base = f"{IIIF}/{iiif_uuid}/full"
    if max(width, height) <= geometry.IIIF_MAX_SIDE:
        return f"{base}/max/0/default.jpg"
    box = f"{geometry.IIIF_MAX_SIDE}," if width >= height else f",{geometry.IIIF_MAX_SIDE}"
    return f"{base}/{box}/0/default.jpg"


def _fetch_one(client: http.PacedClient, uuid: str) -> dict:
    """One object's outcome: a row's worth of fields, or `{"skip": why}`.
    Only `http.SourceAborted` escapes."""
    obj = client.get_json(f"{OBJECT}/{uuid}", headers=LINKED_ART)

    image = _main_image(obj)
    if image is None:
        return {"skip": "no overall image"}
    media_uuid, iiif_uuid = image
    facts = _image_facts(client.get_json(f"{MEDIA}/{media_uuid}", headers=LINKED_ART), iiif_uuid)
    if facts.get("skip"):
        return facts
    width, height = geometry.fit_within(facts["width"], facts["height"])

    production = obj.get("produced_by") or {}
    people = _persons(production)
    names = _statements(production, "/producer-name") or [p.get("_label", "") for p in people]
    names = [n for n in names if n]
    nationalities = _statements(production, "/nationality-and-dates")
    roles = [r.lower() for p in people for r in _statements(p, "/producer-role-statement")]
    culture = next(iter(_culture_statements(obj)), "")
    place = next(iter(_place_statements(obj)), "")

    region = (
        next(filter(None, map(regions.region_from_text, nationalities)), None)
        or regions.region_from_text(culture)
        or regions.region_from_text(place)
    )
    if region is None:
        return {"skip": f"no region from {nationalities or culture or place!r}"}

    date = ""
    for name in ((production.get("timespan") or {}).get("identified_by")) or []:
        if name.get("content"):
            date = text.collapse(name["content"])
            break

    page = next(
        (
            s["id"]
            for s in obj.get("subject_of") or []
            if s.get("format") == "text/html" and s.get("id", "").startswith("https://www.getty.edu/")
        ),
        "",
    )
    tags = []
    for shown in obj.get("shows") or []:
        for term in shown.get("about") or []:
            label = text.collapse(term.get("_label"))
            if label and label not in tags:
                tags.append(label)

    sole_plain_maker = len(names) == 1 and (not roles or roles == [_PLAIN_ROLE])
    return {
        "region": region,
        "width": width,
        "height": height,
        "image_url": _image_url(iiif_uuid, facts["width"], facts["height"]),
        "details_url": page,
        "title": _name_of(obj, "Preferred Title") or _name_of(obj, "Alternate Title") or "Untitled",
        "byline": text.byline(" and ".join(names), date),
        "origin": culture or place,
        "tags": tags,
        "artist": text.maker(names[0]) if sole_plain_maker else "",
    }


def _culture_statements(obj: dict) -> list[str]:
    return [
        text.collapse(r["content"])
        for r in obj.get("referred_to_by") or []
        if r.get("_label") == "Culture Statement" and r.get("content")
    ]


def _place_statements(obj: dict) -> list[str]:
    return [
        text.collapse(r["content"])
        for r in obj.get("referred_to_by") or []
        if r.get("_label") == "Place Created" and r.get("content")
    ]


def fetch(
    client: http.PacedClient,
    cache_dir: str,
    refresh: bool,
    csv_path: str | None = None,  # unused; part of every source's shared signature
) -> Iterator[dict]:
    cache = http.JsonCache(os.path.join(cache_dir, "objects"))
    for uuid in _candidates(client):
        record = None if refresh else cache.get(uuid)
        if record is None:
            try:
                record = _fetch_one(client, uuid)
            except http.SourceAborted:
                raise
            except Exception as e:  # noqa: BLE001 — one bad object must not sink the pass
                record = {"skip": f"error: {e}"}
            cache.put(uuid, record)
        if record.get("skip") or not geometry.meets_minimum(record["width"], record["height"]):
            continue
        yield {"source": "getty", "id": uuid, **{k: record[k] for k in _ROW_FIELDS}}


_ROW_FIELDS = (
    "region", "width", "height", "image_url", "details_url",
    "title", "byline", "origin", "tags", "artist",
)
