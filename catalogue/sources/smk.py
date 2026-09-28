"""SMK — Statens Museum for Kunst, Denmark's national gallery.

One paged search (`object_names:maleri`, Danish for "painting") returns every
field this source needs, so — like Cleveland — there's no per-object request
to cache; a full pass is three 2000-row pages.

The search deliberately never passes `lang=en`: verified against the live
API, adding it makes `filters=...,[object_names:maleri]` match nothing
(`found` drops from thousands to 0), seemingly because the requested
language no longer lines up with the Danish facet value. So titles and
nationalities come back in Danish — `regions.region_from_danish_nationality`
is the region side of that; `title` here just takes whatever `titles` entry
is closest to English, or the first if none is tagged that way.
"""

from typing import Iterator
from urllib.parse import urlencode

from .. import geometry, http, regions

BASE = "https://api.smk.dk/api/v1/art/search/"
FILTERS = "[public_domain:true],[has_image:true],[object_names:maleri]"
PAGE_SIZE = 2000

HOST_GAPS = {"api.smk.dk": 1.0}


def _byline(name: str, period: str) -> str:
    name, period = name.strip(), period.strip()
    if name and period:
        return f"{name}, {period}"
    return name or period


def _title(titles: list[dict]) -> str:
    if not titles:
        return ""
    for entry in titles:
        if "engl" in (entry.get("language") or "").lower():
            return (entry.get("title") or "").strip()
    return (titles[0].get("title") or "").strip()


def _production(item: dict) -> tuple[str, str, str]:
    """`(creator name, nationality, byline date period)` from the first
    `production` entry, or all empty if there isn't one."""
    production = item.get("production") or []
    if not production:
        return "", "", ""
    entry = production[0]
    forename = (entry.get("creator_forename") or "").strip()
    surname = (entry.get("creator_surname") or "").strip()
    name = f"{forename} {surname}".strip() or (entry.get("creator") or "").strip()
    nationality = (entry.get("creator_nationality") or "").strip()
    dates = item.get("production_date") or []
    period = (dates[0].get("period") or "").strip() if dates else ""
    return name, nationality, period


def _tags(item: dict) -> list[str]:
    # `content_description` looked like a second tag field on paper, but a
    # live sample shows it holds full Danish caption sentences ("Motivet
    # viser Kählers fabrik set fra Banken i Næstved"), not terms — including
    # it would put prose in the same column every other source uses for
    # short subject keywords. `content_subject` really is short place/subject
    # words ("Bretagne", "Næstved"), so only that one is used.
    value = item.get("content_subject")
    if isinstance(value, list):
        return [v.strip() for v in value if isinstance(v, str) and v.strip()]
    return []


def _image(item: dict) -> tuple[str, int, int] | None:
    iiif_id = item.get("image_iiif_id")
    try:
        width, height = int(item["image_width"]), int(item["image_height"])
    except (KeyError, ValueError, TypeError):
        width = height = None
    if iiif_id and width and height:
        w, h = geometry.fit_within(width, height)
        return f"{iiif_id}/full/!4000,4000/0/default.jpg", w, h
    native = item.get("image_native")
    if native and width and height:
        return native, width, height
    return None


def fetch(
    client: http.PacedClient,
    cache_dir: str,  # unused; part of every source's shared signature
    refresh: bool,  # unused: nothing here is cached, see module docstring
    csv_path: str | None = None,
) -> Iterator[dict]:
    offset = 0
    while True:
        query = urlencode({"keys": "*", "filters": FILTERS, "offset": offset, "rows": PAGE_SIZE})
        page = client.get_json(f"{BASE}?{query}")
        items = page.get("items") or []
        if not items:
            break

        for item in items:
            if not item.get("public_domain"):
                continue
            names = {n.get("name", "").lower() for n in (item.get("object_names") or [])}
            if "maleri" not in names:
                continue
            image = _image(item)
            if image is None or not geometry.meets_minimum(image[1], image[2]):
                continue
            image_url, width, height = image

            object_number = item.get("object_number")
            if not object_number:
                continue

            name, nationality, period = _production(item)

            yield {
                "source": "smk",
                "id": object_number,
                "region": regions.region_from_danish_nationality(nationality),
                "width": width,
                "height": height,
                "image_url": image_url,
                "details_url": item.get("frontend_url") or "",
                "title": _title(item.get("titles") or []) or "Untitled",
                "byline": _byline(name, period),
                "origin": nationality,
                "tags": _tags(item),
            }

        offset += PAGE_SIZE
        if offset >= page.get("found", 0):
            break
