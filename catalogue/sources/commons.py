"""Wikimedia Commons paintings, one named artist at a time.

Unlike the four museum sources, this one isn't a museum's own open-access
API — it's the MediaWiki API (`w/api.php`) pointed at whichever categories
`catalogue/artists.json` lists for an artist. Every row it yields carries an
`artist` display name (the file's 12th TSV column); every other source
leaves that column empty. To add an artist, add one entry to
`artists.json` — see `build.py`'s module docstring for the shape.

For each artist, every listed category is walked recursively
(`_walk_all_categories`; see `MAX_CATEGORY_DEPTH` and `DEEP_BRANCH_DEPTH`),
skipping any subcategory whose name matches `SKIP_SUBCATEGORY_WORDS` — a
coarse filter against exhibition photos, building/interior shots, sketches,
stamps and the like, which turn up constantly under a painter's category
tree but aren't paintings. Files come from `generator=categorymembers&gcmtype=file`
in batches of `BATCH_SIZE`, with `prop=imageinfo|categories` fetching
everything needed (URL, pixel size, mime, licence metadata, and the file's
own categories, reused as `tags`) in the same request. A file surviving that
can still fail `_non_painting_reason` — the same word list, applied to the
file itself rather than the branch it was found in, since a mosaic or a
majolica fireplace panel Vrubel designed sits in Commons' tree right next to
the painting it reproduces. Every rejection, from either check, is logged to
`<cache_dir>/rejections.tsv` by `_write_rejections_log`.

Several Commons scans of one painting are common (a full view, a museum's
own reproduction, a Google Art Project upload) and are collapsed to the
largest by `_dedupe_by_painting`, grouping by whichever of three signals two
files share: a Wikidata item (from structured data, batch-fetched by
`_wikidata_depicted_work` after the walk), a category specific to that one
painting, or the same title at the same proportions. A title alone is no
evidence: two different paintings sharing a title happens often enough
(translations, "Untitled") that it isn't safe.

The walk also keeps out of places: a subcategory named for a museum, street,
monument, tomb, garden or the like (`PLACE_WORDS`, `PLACE_PHRASES`) is not
entered, since it holds visitors' photographs rather than the painter's
work; and a byline year later than the painter's `died` year in
`artists.json` is dropped, being the date of a photograph.

Sizing is the one place this differs from the museums. `iiurlwidth` bounds
only a thumbnail's *width*, so a single batch request asking for
`iiurlwidth=MAX_LONG_SIDE` correctly caps a landscape image's long side, but
does nothing for a portrait one — its width is usually already under the
cap, so the thumbnailer just hands back the untouched original, height and
all. A portrait past the cap therefore gets one extra per-file request
asking for the width that, scaled by its own aspect ratio, lands its height
(the actual long side) at `MAX_LONG_SIDE` — see `_thumb_width_for_long_side`.
An image already within the cap on both axes skips thumbnailing entirely and
uses the original URL.

Every API response — category listings, file batches, the odd per-file
thumbnail request — is cached by request URL under `<cache_dir>/api/`, so a
rerun with the same categories costs nothing.
"""

import hashlib
import html
import json
import math
import os
import re
import sys
import time
from typing import Iterator
from urllib.parse import urlencode

from .. import geometry, http, regions

API = "https://commons.wikimedia.org/w/api.php"

HOST_GAPS = {"commons.wikimedia.org": 1.0}

ARTISTS_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.realpath(__file__))), "artists.json")

BATCH_SIZE = 50
SDC_BATCH_SIZE = 50

# The default depth this source walks from an artist's listed root
# categories. A navigational branch that fans out into one subcategory per
# painting (its name matching `DEEP_BRANCH_MARKERS`) is re-walked from
# there, this much deeper again — Commons buries "Lilac (1901)" or "The
# Pearl Oyster" several levels under "... by name" or "... by museum", well
# past what a flat walk from the artist's own root would reach.
MAX_CATEGORY_DEPTH = 3
DEEP_BRANCH_DEPTH = 5
DEEP_BRANCH_MARKERS = ("by name", "by museum")

# Not an IIIF request — Commons thumbnails work differently — but this
# catalogue bounds every source's images to the same long-side cap, so this
# is `geometry.IIIF_MAX_SIDE` under a name that doesn't claim IIIF.
MAX_LONG_SIDE = geometry.IIIF_MAX_SIDE

# Words meaning "not a painting" (or, for the last few, "not a *finished*
# painting" — a preparatory stage worth excluding rather than its own row,
# per policy), checked case-insensitively as a whole word against two
# different things: a *subcategory's* own name, while deciding whether to
# walk into it at all (`_looks_unwanted`), and a *file's* own title and
# categories, even for a file reached through a category this source does
# walk (`_non_painting_reason`) — a Vrubel-designed mosaic or majolica
# fireplace panel sits right next to the painting it reproduces in Commons'
# own tree ("Mosaic ... on Metropol Hotel" shares a category with the
# "Princess of Dreams" panel it depicts). Deliberately does *not* include
# camera-EXIF or "somebody's own photo" categories such as "Taken with
# <camera>": a museum photo contributor's own high-quality photograph of a
# real, finished painting carries exactly the same signal as a tourist's
# photo of a building, so it isn't actually evidence either way — a photo of
# a building is instead caught by "building"/"hotel"/"facade"/... below,
# on the file's own title or categories, same as everything else here.
NON_PAINTING_WORDS = (
    "exhibition", "exhibitions", "interior", "interiors", "building", "buildings",
    "photograph", "photographs", "drawing", "drawings", "stamp", "stamps",
    "postcard", "postcards", "frame", "frames", "reproduction", "reproductions",
    "graphic", "graphics", "hotel", "hotels", "facade", "facades",
    "mosaic", "mosaics", "fireplace", "fireplaces", "ceramic", "ceramics",
    "majolica", "majolicas", "mayolica", "mayolicas", "ornament", "ornaments",
    "installation", "installations", "sculpture", "sculptures",
    "poster", "posters", "grave", "graves",
    "sketch", "sketches", "fragment", "fragments", "detail", "details",
    "placa", "legenda", "label", "labels", "signature", "mausoléu", "mausoleum",
    "estudo", "estudos", "esboço", "lithograph", "lithographs", "litho",
    "lithography", "engraving", "engravings", "etching", "etchings", "woodcut",
    "woodcuts", "atelier", "firma", "portón", "composicion", "charivari",
    "herrick", "print", "prints",
    "study", "studies", "эскиз", "эскизы", "фрагмент", "фрагменты",
)
# Multi-word phrases doing the same job as `NON_PAINTING_WORDS`, checked as
# a plain substring since "\b...\b" can't span a space-separated phrase.
NON_PAINTING_PHRASES = (
    "set design", "set designs",
    # Engravings lifted out of a scanned book or periodical (Thomas Baines's
    # Zambezi plates in "Le Tour du monde"; "Extracted images" is too broad,
    # it also tags real paintings), not paintings.
    "périodique", "of le tour du monde", "book cover", "title page",
    "titlepage", "table of contents", "(book)", "century copy", "(page ",
    "paris studio", "portret van kunstenaar", "emerald hours",
    "botanical illustrations", "manuscript pages", "international mission photography",
    "day & son", "portrait of the painter", "and family",
)

# Words that mark a *subcategory* as about a place, an institution or a thing
# named after the painter rather than his paintings — the museum, a street,
# his tomb, a monument, a garden — and so as full of visitors' photographs.
# Only the subcategory's name is tested, never a file's own categories: a
# genuine painting is routinely tagged with the museum that holds it. A name
# containing "paintings" or "works" is exempt ("Paintings in the Museu X"
# holds paintings).
PLACE_WORDS = (
    "museum", "museums", "museu", "museo", "monument", "monuments", "monumento",
    "statue", "statues", "estatua", "calle", "street", "rua", "praça", "plaza",
    "mausoleum", "mausoléu", "tomb", "tombs", "school", "schools", "escola",
    "garden", "gardens", "jardín", "jardim", "banknote", "banknotes", "plaque",
    "plaques", "teatro", "theatre", "prefeitura", "mayors", "salão", "espaço",
    "fundação", "pinacoteca", "quinta", "feira", "audio", "catalogs", "catalogues",
    "diccionario", "restauration", "restoration", "symbols", "municipality",
    "secretaries", "correios", "paço", "casa", "heritage",
)
# Phrases doing the same job: portraits *of* the painter by others, replicas
# and copies after him, tiled crops of one painting, books about or by him.
PLACE_PHRASES = (
    "things named after", "works after", "portraits of", "portrait of ",
    "in art", "tile set", "life and work", "narrative of", "shifts and expedients",
    "freundschaftsgalerie", "replicas of", "by laurindo",
)

SKIP_SUBCATEGORY_WORDS = NON_PAINTING_WORDS

# A category names one specific *work* — safe to group Commons scans by in
# `_dedupe_by_painting` — only when it explicitly attributes that work to
# this artist, following one of Commons' own two conventions: "<Title> by
# <Artist>" (e.g. "Pan by Vrubel") or "<Title> (<Artist ...>)" (e.g. "Demon
# Seated (Mikhail Vrubel)"). This one requirement is also what rules out
# every kind of hub category without having to name each one: a licensing
# tag, an upload-source category ("Files by User:X", "Google Art Project
# works by Mikhail Vrubel" — the artist's name sits after "works", not the
# title), a year or subject bucket ("Lilacs in art"), or a person the artist
# painted more than once ("Nadezhda Zabela-Vrubel", his wife) — none of
# these name a specific work, so none of them can accidentally merge two
# different ones just for mentioning the artist somewhere. The one thing
# the grammar alone can't rule out is the same "<Genre> by <Artist>" pattern
# used for a whole genre rather than one work ("Portraits by Mikhail
# Vrubel"); `_GENRE_HUB_HEADS` is that short, explicit exception list.
_GENRE_HUB_HEADS = frozenset({
    "paintings", "works", "drawings", "portraits", "self-portraits",
    "posters", "sculptures", "mayolicas", "majolicas", "religious works",
    "set designs",
})


def _is_specific_work_category(name: str, artist_name: str) -> bool:
    lowered = name.lower()
    surname = artist_name.split()[-1].lower()

    by_match = re.search(r"^(.*?)\bby (?:" + re.escape(surname) + "|" + re.escape(artist_name.lower()) + r")\b", lowered)
    if by_match:
        head = by_match.group(1).strip()
    else:
        paren_match = re.search(r"\(([^)]*)\)", lowered)
        if not (paren_match and surname in paren_match.group(1)):
            return False
        head = lowered[: paren_match.start()].strip()

    if not head:
        return False
    return not any(head == hub or head.endswith(" " + hub) for hub in _GENRE_HUB_HEADS)

_PD_MARKERS = ("public domain", "cc0")
_TAG_RE = re.compile(r"<[^>]+>")
_YEAR_RE = re.compile(r"\b(1[0-9]{3}|20[0-9]{2})\b")


def _load_artists() -> list[dict]:
    with open(ARTISTS_PATH, encoding="utf-8") as f:
        artists = json.load(f)
    for artist in artists:
        if not isinstance(artist.get("died"), int):
            raise ValueError(f"{ARTISTS_PATH}: {artist.get('name')!r} needs an integer \"died\" year")
        if artist.get("region") not in regions.REGIONS:
            raise ValueError(
                f"{ARTISTS_PATH}: {artist.get('name')!r} has region "
                f"{artist.get('region')!r}, not one of {regions.REGIONS}"
            )
    return artists


def _strip_html(text: str | None) -> str:
    if not text:
        return ""
    return " ".join(html.unescape(_TAG_RE.sub("", text)).split())


def _meta(extmetadata: dict, key: str) -> str:
    return (extmetadata.get(key) or {}).get("value") or ""


# Some files' ObjectName is a clean title with a QuickStatements
# multi-language label or title dump (`label QS:Lxx,"..."` or
# `title QS:Pnnnn,xx:"..."`, once per language) pasted straight after it
# with no separator — a Commons data-entry mistake seen live on this
# artist's own files ("Panlabel QS:Lfr,\"Pan\"...", "...Demon (sitting)title
# QS:P1476,ru:..."), not something `_strip_html` can catch since no HTML tag
# is involved. No `\b` before the marker: the concatenation runs the real
# title's last letter straight into "label"/"title" with no word boundary
# between them. Only the text before the first such marker is kept.
_QS_LABEL_JUNK_RE = re.compile(r"(?:label|title) QS:\S*?,")


# A leading language label ("Portuguese: Porto de Santos") and, in the
# "Title, by Painter, 1885, oil on canvas - Gallery - City - DSC08793"
# file-name pattern, everything after the title.
_LANGUAGE_LABEL_RE = re.compile(r"^(?:English|Portuguese|Spanish|French|German|Italian|Dutch|Latin|Russian)\s*:\s*")
# A Russian title glued to its English one: "«Демон (сидящий)»Demon (sitting)".
_RUSSIAN_QUOTED_RE = re.compile(r"^«[^»]*»")
_BY_PAINTER_RE = re.compile(r",\s+by\s+.*$", re.IGNORECASE)


def _clean_object_name(text: str) -> str:
    text = _QS_LABEL_JUNK_RE.split(text, maxsplit=1)[0].strip()
    text = _RUSSIAN_QUOTED_RE.sub("", _LANGUAGE_LABEL_RE.sub("", text)).strip() or text
    return _BY_PAINTER_RE.sub("", text).strip() or text


def _looks_unwanted(category_title: str) -> bool:
    name = category_title.split(":", 1)[-1].lower()
    if not re.search(r"\b(?:paintings|works)\b", name):
        if any(re.search(r"\b" + re.escape(word) + r"\b", name) for word in PLACE_WORDS):
            return True
        if any(phrase in name for phrase in PLACE_PHRASES):
            return True
    if any(re.search(r"\b" + re.escape(word) + r"\b", name) for word in SKIP_SUBCATEGORY_WORDS):
        return True
    return any(phrase in name for phrase in NON_PAINTING_PHRASES)


# A camera's own file name or a photograph's date stamp standing in for a
# title ("IMG 20211130 140351", "... 20221021 133627", "(2024-10-26)"): the
# file is somebody's photograph of a place or a gallery wall, and a
# painting's year never looks like this.
_PHOTO_NAME_RE = re.compile(r"\bimg[ _]?\d{6,}|\b(?:19|20)\d{6}\b|\(\d{4}-\d{2}-\d{2}\)", re.IGNORECASE)


def _non_painting_reason(file_title: str, tags: list[str], title: str = "") -> str | None:
    """Why this file itself — as opposed to the category it was found in —
    is something other than a finished painting: a photo of a building,
    mosaic or ceramic object Vrubel decorated, a museum installation shot, a
    sketch or study, or the like. `None` means it passes. Checked per file
    because a category this source does walk can hold both a painting and,
    say, a photo of the building a mosaic version of it was later installed
    on. Deliberately never looks at camera EXIF or "Taken with .../Files by
    ..." categories — see `NON_PAINTING_WORDS`'s doc for why that signal
    doesn't actually distinguish a photo of a painting from a photo of
    anything else."""
    for text in (file_title, title):
        if _PHOTO_NAME_RE.search(text):
            return "photo-name"
    # The uploader's own photograph (CC0, "Self-published work") with nothing
    # marking it as a reproduction of an artwork.
    lowered_tags = [tag.lower() for tag in tags]
    if "self-published work" in lowered_tags and not any(
        tag.startswith(("artworks", "pd-art")) or "paintings" in tag for tag in lowered_tags
    ):
        return "self-published photo"
    for text in (file_title, title, *tags):
        lowered = text.lower()
        for word in NON_PAINTING_WORDS:
            if re.search(r"\b" + re.escape(word) + r"\b", lowered):
                return f"word:{word}"
        for phrase in NON_PAINTING_PHRASES:
            if phrase in lowered:
                return f"phrase:{phrase}"
    return None


def _get_json_with_maxlag_retry(client: http.PacedClient, url: str, max_attempts: int = 5):
    """`maxlag=5` on every request asks the API to say so, as a normal JSON
    body (`{"error": {"code": "maxlag", ...}}`), rather than refuse outright
    — `http.PacedClient` only retries on HTTP 403/429, so that case is
    handled here instead, one artist's worth of requests being far too few
    to reach `SourceAborted` from ordinary throttling."""
    wait = 5.0
    for _ in range(max_attempts):
        data = client.get_json(url)
        error = data.get("error") if isinstance(data, dict) else None
        if error and error.get("code") == "maxlag":
            print(f"  commons.wikimedia.org -> maxlag, waiting {wait:.0f}s", file=sys.stderr)
            time.sleep(wait)
            wait *= 2
            continue
        return data
    raise http.SourceAborted("commons.wikimedia.org kept reporting maxlag")


def _cached_get_json(client: http.PacedClient, cache: http.JsonCache, refresh: bool, url: str):
    key = hashlib.sha1(url.encode()).hexdigest()
    if not refresh:
        cached = cache.get(key)
        if cached is not None:
            return cached
    data = _get_json_with_maxlag_retry(client, url)
    cache.put(key, data)
    return data


def _subcategories(client: http.PacedClient, cache: http.JsonCache, refresh: bool, title: str) -> list[str]:
    titles: list[str] = []
    params = {
        "action": "query", "format": "json", "list": "categorymembers",
        "cmtitle": title, "cmtype": "subcat", "cmlimit": "500", "maxlag": "5",
    }
    cont: dict = {}
    while True:
        query = {**params, **cont}
        data = _cached_get_json(client, cache, refresh, f"{API}?{urlencode(query)}")
        for member in (data.get("query") or {}).get("categorymembers", []):
            if member.get("title"):
                titles.append(member["title"])
        cont = data.get("continue")
        if not cont:
            break
    return titles


def _walk_categories(
    client: http.PacedClient, cache: http.JsonCache, refresh: bool, root_title: str, max_depth: int
) -> list[str]:
    """`root_title` plus every subcategory reachable from it within
    `max_depth` levels — except a branch whose own name looks unwanted,
    which is dropped along with everything under it."""
    kept = [root_title]
    frontier = [root_title]
    for _ in range(max_depth):
        next_frontier = []
        for title in frontier:
            for sub in _subcategories(client, cache, refresh, title):
                if sub in kept or _looks_unwanted(sub):
                    continue
                kept.append(sub)
                next_frontier.append(sub)
        frontier = next_frontier
        if not frontier:
            break
    return kept


def _walk_all_categories(client: http.PacedClient, cache: http.JsonCache, refresh: bool, root_name: str) -> list[str]:
    """Every category to pull files from for one of an artist's listed
    roots: `root_name` walked at `MAX_CATEGORY_DEPTH`, plus — for any branch
    found there whose name matches `DEEP_BRANCH_MARKERS` — that branch
    walked `DEEP_BRANCH_DEPTH` levels deeper still, since a "by name" or "by
    museum" branch is where Commons buries one subcategory per painting,
    often past what a flat walk from the artist's own root would reach."""
    categories = _walk_categories(client, cache, refresh, f"Category:{root_name}", MAX_CATEGORY_DEPTH)
    seen = set(categories)
    for category in list(categories):
        if not any(marker in category.lower() for marker in DEEP_BRANCH_MARKERS):
            continue
        for deeper in _walk_categories(client, cache, refresh, category, DEEP_BRANCH_DEPTH):
            if deeper not in seen:
                seen.add(deeper)
                categories.append(deeper)
    return categories


def _iter_file_pages(client: http.PacedClient, cache: http.JsonCache, refresh: bool, category_title: str) -> Iterator[dict]:
    params = {
        "action": "query", "format": "json",
        "generator": "categorymembers", "gcmtitle": category_title,
        "gcmtype": "file", "gcmlimit": str(BATCH_SIZE),
        "prop": "imageinfo|categories",
        "iiprop": "url|size|mime|extmetadata",
        "iiurlwidth": str(MAX_LONG_SIDE),
        "cllimit": "max", "maxlag": "5",
    }
    cont: dict = {}
    while True:
        query = {**params, **cont}
        data = _cached_get_json(client, cache, refresh, f"{API}?{urlencode(query)}")
        pages = (data.get("query") or {}).get("pages") or {}
        yield from pages.values()
        cont = data.get("continue")
        if not cont:
            break


def _thumb_width_for_long_side(width: int, height: int) -> int:
    """The `iiurlwidth` to request so a portrait image's *height* — its long
    side — comes back at `MAX_LONG_SIDE`. Floored, not rounded, so the
    thumbnailer's own width-to-height rounding can't push the actual long
    side a pixel back over the cap."""
    return max(1, math.floor(MAX_LONG_SIDE * width / height))


def _fetch_thumbnail(
    client: http.PacedClient, cache: http.JsonCache, refresh: bool, file_title: str, want_width: int
) -> tuple[str, int, int] | None:
    params = {
        "action": "query", "format": "json", "titles": file_title,
        "prop": "imageinfo", "iiprop": "url", "iiurlwidth": str(want_width), "maxlag": "5",
    }
    data = _cached_get_json(client, cache, refresh, f"{API}?{urlencode(params)}")
    for page in ((data.get("query") or {}).get("pages") or {}).values():
        info = (page.get("imageinfo") or [None])[0]
        if info and info.get("thumburl") and info.get("thumbwidth") and info.get("thumbheight"):
            return info["thumburl"], info["thumbwidth"], info["thumbheight"]
    return None


def _first_item_qid(statements: list[dict] | None) -> str | None:
    for statement in statements or []:
        value = ((statement.get("mainsnak") or {}).get("datavalue") or {}).get("value")
        if isinstance(value, dict) and value.get("id"):
            return value["id"]
    return None


def _wikidata_depicted_work(
    client: http.PacedClient, cache: http.JsonCache, refresh: bool, pageids: list[str]
) -> dict[str, str]:
    """The Wikidata item each of `pageids` structured data says it is a
    digital representation of (`P6243`) or depicts (`P180`, fallback) — from
    Commons' own Wikibase repository, where a file's structured data lives
    under the entity id `M<pageid>`. Several scans of one painting reliably
    share this even when their titles and categories don't agree. A file
    carrying neither claim is simply absent from the result;
    `_dedupe_by_painting` falls back to a shared category or an identical
    title for those."""
    result: dict[str, str] = {}
    for i in range(0, len(pageids), SDC_BATCH_SIZE):
        batch = pageids[i:i + SDC_BATCH_SIZE]
        mids = "|".join(f"M{pid}" for pid in batch)
        params = {"action": "wbgetentities", "format": "json", "ids": mids, "props": "claims", "maxlag": "5"}
        data = _cached_get_json(client, cache, refresh, f"{API}?{urlencode(params)}")
        entities = data.get("entities") or {}
        for pid in batch:
            statements = (entities.get(f"M{pid}") or {}).get("statements") or {}
            qid = _first_item_qid(statements.get("P6243")) or _first_item_qid(statements.get("P180"))
            if qid:
                result[pid] = qid
    return result


def _dedupe_by_painting(
    rows: list[dict], qid_by_id: dict[str, str], artist_name: str
) -> tuple[list[dict], dict[str, str]]:
    """Collapses several Commons scans of one painting into the largest.
    Two rows are unioned into the same group only if they share a Wikidata
    item (`qid_by_id`), a category naming that one work specifically
    (`_is_specific_work_category`), or a title *and* an aspect ratio (to
    5%). A title alone is not evidence — different paintings share titles —
    but a title plus proportions, within one artist, is.

    Returns the kept rows plus a `{dropped_id: kept_id}` map, so the caller
    can log exactly which id each dropped row was folded into."""
    parent = {row["id"]: row["id"] for row in rows}

    def find(x: str) -> str:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a: str, b: str) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb

    first_seen: dict[tuple, str] = {}
    for row in rows:
        keys = []
        if row["id"] in qid_by_id:
            keys.append(("qid", qid_by_id[row["id"]]))
        keys.extend(
            ("cat", tag) for tag in row["tags"] if _is_specific_work_category(tag, artist_name)
        )
        # Same title and nearly the same proportions: another scan of one
        # painting (with and without frame is not caught; that is a different
        # ratio). Ratio is part of the key so two paintings sharing a bare
        # title such as "Paisaje" are not merged.
        keys.append(("title", " ".join(row["title"].lower().split()), round(20 * row["width"] / row["height"])))
        for key in keys:
            if key in first_seen:
                union(row["id"], first_seen[key])
            else:
                first_seen[key] = row["id"]

    groups: dict[str, list[dict]] = {}
    for row in rows:
        groups.setdefault(find(row["id"]), []).append(row)

    kept = []
    dropped_into: dict[str, str] = {}
    for group in groups.values():
        winner = max(group, key=lambda r: r["width"] * r["height"])
        kept.append(winner)
        for row in group:
            if row["id"] != winner["id"]:
                dropped_into[row["id"]] = winner["id"]
    return kept, dropped_into


def _is_public_domain(meta: dict) -> bool:
    licence = _strip_html(_meta(meta, "LicenseShortName")).lower()
    if any(marker in licence for marker in _PD_MARKERS) or re.search(r"\bpd\b", licence):
        return True
    return _meta(meta, "Copyrighted") == "False"


def _fallback_title(file_title: str, artist_name: str) -> str:
    name = os.path.splitext(file_title.split(":", 1)[-1])[0]
    prefix = f"{artist_name} - "
    if name.startswith(prefix):
        name = name[len(prefix):]
    return name.strip()


def _byline(artist_name: str, meta: dict, died: int) -> str:
    """The painter and the year of the work. A year after the painter's death
    is the date of a photograph of it, not of the painting, so it is dropped
    rather than printed."""
    match = _YEAR_RE.search(_strip_html(_meta(meta, "DateTimeOriginal")))
    return f"{artist_name}, {match.group(1)}" if match and int(match.group(1)) <= died else artist_name


def _tags(page: dict) -> list[str]:
    tags = []
    for cat in page.get("categories") or []:
        name = (cat.get("title") or "").split(":", 1)[-1].strip()
        if name:
            tags.append(name)
    return tags


def _fetch_one(
    client: http.PacedClient, cache: http.JsonCache, refresh: bool, page: dict, artist: dict
) -> tuple[dict | None, str | None]:
    """One file's row, or `(None, reason)` if it fails a gate. Never raises
    for an ordinary rejection — only `http.SourceAborted` escapes."""
    pageid = page.get("pageid")
    file_title = page.get("title") or ""
    infos = page.get("imageinfo") or []
    if pageid is None or not infos:
        return None, "no imageinfo"
    info = infos[0]

    if (info.get("mime") or "").lower() != "image/jpeg":
        return None, f"mime:{info.get('mime')}"
    width, height = info.get("width"), info.get("height")
    if not isinstance(width, int) or not isinstance(height, int):
        return None, "no pixel size"
    if not geometry.meets_minimum(width, height):
        return None, f"size:{width}x{height}"

    meta = info.get("extmetadata") or {}
    if not _is_public_domain(meta):
        return None, f"licence:{_strip_html(_meta(meta, 'LicenseShortName')) or '(none)'}"

    tags = _tags(page)
    title = _clean_object_name(_strip_html(_meta(meta, "ObjectName"))) or _fallback_title(file_title, artist["name"])
    reason = _non_painting_reason(file_title, tags, title)
    if reason is not None:
        return None, reason

    long_side = max(width, height)
    if long_side <= MAX_LONG_SIDE:
        image_url, final_w, final_h = info.get("url"), width, height
    elif width >= height:
        image_url, final_w, final_h = info.get("thumburl"), info.get("thumbwidth"), info.get("thumbheight")
    else:
        thumb = _fetch_thumbnail(client, cache, refresh, file_title, _thumb_width_for_long_side(width, height))
        if thumb is None:
            return None, "thumbnail request failed"
        image_url, final_w, final_h = thumb
    if not image_url or not final_w or not final_h:
        return None, "no usable image URL"

    row = {
        "source": "wmc",
        "id": str(pageid),
        "region": artist["region"],
        "width": final_w,
        "height": final_h,
        "image_url": image_url,
        "details_url": info.get("descriptionurl") or "",
        "title": title,
        "byline": _byline(artist["name"], meta, artist["died"]),
        "origin": "",
        "tags": tags,
        "artist": artist["name"],
    }
    return row, None


def _write_rejections_log(cache_dir: str, rejections: list[tuple[str, str, str]]) -> None:
    """A debug dump of every file this run looked at but didn't keep, one
    row per `(pageid, file title, reason)` — overwritten each run, since
    it describes this run's decisions rather than anything to resume."""
    path = os.path.join(cache_dir, "rejections.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("pageid\ttitle\treason\n")
        for pageid, title, reason in rejections:
            f.write(f"{pageid}\t{title}\t{reason}\n")


def fetch(
    client: http.PacedClient,
    cache_dir: str,
    refresh: bool,
    csv_path: str | None = None,  # unused; part of every source's shared signature
) -> Iterator[dict]:
    cache = http.JsonCache(os.path.join(cache_dir, "api"))
    rejections: list[tuple[str, str, str]] = []
    # Across artists, not per artist: a file reachable from two painters'
    # category trees (a portrait of one by the other) must become one row, or
    # the TSV carries the same (source, id) twice. The first artist listed wins.
    seen: set[int] = set()
    for artist in _load_artists():
        candidates: list[dict] = []
        for root in artist["categories"]:
            for category in _walk_all_categories(client, cache, refresh, root):
                for page in _iter_file_pages(client, cache, refresh, category):
                    pageid = page.get("pageid")
                    if pageid is None or pageid in seen:
                        continue
                    seen.add(pageid)
                    file_title = page.get("title") or ""
                    try:
                        row, reason = _fetch_one(client, cache, refresh, page, artist)
                    except http.SourceAborted:
                        raise
                    except Exception as e:  # noqa: BLE001 — one bad file must not sink the pass
                        row, reason = None, f"error: {e}"
                    if row is not None:
                        candidates.append(row)
                    else:
                        rejections.append((str(pageid), file_title, reason or "unknown"))
        # Grouping needs every candidate's Wikidata claim in hand at once, so
        # this batches after the walk rather than checking file by file.
        qid_by_id = _wikidata_depicted_work(client, cache, refresh, [row["id"] for row in candidates])
        kept, dropped_into = _dedupe_by_painting(candidates, qid_by_id, artist["name"])
        by_id = {row["id"]: row for row in candidates}
        for dropped_id, winner_id in dropped_into.items():
            rejections.append((dropped_id, by_id[dropped_id]["title"], f"deduped:kept={winner_id}"))
        yield from kept
    _write_rejections_log(cache_dir, rejections)
