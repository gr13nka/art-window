"""The Cleveland Museum of Art's open-access paintings.

One paged API (`type=Painting&cc0=1&has_image=1`) returns every field this
source needs in the listing itself — there's no per-object follow-up request,
so there's nothing to cache per object either; a full pass is a handful of
1000-row pages, cheap enough to redo from scratch every run rather than trust
a stale manifest.

The API has no structured subject/keyword field (checked against a live
sample: `artists_tags` describes the artist, not the picture), so `tags` is
always empty here — honestly, per the shared contract, rather than reaching
for prose fields (`description`, `did_you_know`) that aren't tags at all.
"""

from typing import Iterator
from urllib.parse import urlencode

from .. import geometry, http, regions, text

BASE = "https://openaccess-api.clevelandart.org/api/artworks/"
PAGE_SIZE = 1000

HOST_GAPS = {"openaccess-api.clevelandart.org": 1.0}


def _creator_name(creators: list[dict]) -> str:
    if not creators:
        return ""
    # "John Singleton Copley (American, born ..., 1738–1815)" -> the name.
    return creators[0].get("description", "").split(" (")[0].strip()


def _image(artwork: dict) -> tuple[str, int, int] | None:
    """The best usable JPEG: `print` if it clears the size floor, else `web`
    if that alone does. Neither is assumed to exist — some records carry
    only a TIFF."""
    images = artwork.get("images") or {}
    for variant in ("print", "web"):
        info = images.get(variant) or {}
        url = info.get("url") or ""
        try:
            width, height = int(info["width"]), int(info["height"])
        except (KeyError, ValueError):
            continue
        if url.lower().endswith((".jpg", ".jpeg")) and geometry.meets_minimum(width, height):
            return url, width, height
    return None


def fetch(
    client: http.PacedClient,
    cache_dir: str,  # unused; part of every source's shared signature
    refresh: bool,  # unused: nothing here is cached, see module docstring
    csv_path: str | None = None,
) -> Iterator[dict]:
    skip = 0
    while True:
        query = urlencode(
            {"type": "Painting", "cc0": 1, "has_image": 1, "limit": PAGE_SIZE, "skip": skip}
        )
        page = client.get_json(f"{BASE}?{query}")
        artworks = page.get("data") or []
        if not artworks:
            break

        for artwork in artworks:
            if artwork.get("share_license_status") != "CC0" or artwork.get("type") != "Painting":
                continue
            image = _image(artwork)
            if image is None:
                continue
            image_url, width, height = image

            culture = ", ".join(c for c in (artwork.get("culture") or []) if c)
            creators = artwork.get("creators") or []

            yield {
                "source": "cma",
                "id": str(artwork["id"]),
                "region": regions.region_from_text(culture),
                "width": width,
                "height": height,
                "image_url": image_url,
                "details_url": artwork.get("url") or "",
                "title": (artwork.get("title") or "").strip() or "Untitled",
                "byline": text.byline(_creator_name(creators), artwork.get("creation_date") or ""),
                "origin": culture,
                "tags": [],
            }

        skip += PAGE_SIZE
        total = (page.get("info") or {}).get("total", 0)
        if skip >= total:
            break
