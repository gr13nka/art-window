"""The National Gallery of Art's (Washington) open-access paintings.

Unlike the Met, everything needed already sits in NGA's open-data CSVs —
no live API calls, no per-object network round-trip, just five downloads
(cached under `raw/` and reused as-is; there's nothing to resume mid-object
because there's no per-object request to interrupt) and a handful of joins:

- `objects.csv` — the candidate list (`classification == "Painting"`) and,
  directly, the title and byline (`attribution`, `displaydate` — no need to
  resolve the artist through `constituents.csv` just for a name NGA already
  hands over).
- `published_images.csv` — the primary open-access image
  (`viewtype == "primary"`, `openaccess == "1"`) and its real pixel size.
- `objects_constituents.csv` + `constituents.csv` — the primary artist's
  `nationality`, which is the only reason those two files are read at all:
  region, not byline.
- `objects_terms.csv` — `Keyword`/`Theme`/`Style` terms as tags.

IIIF serves the image (`{iiifurl}/full/!4000,4000/0/default.jpg`), which is
inherently a direct JPEG by construction — the format is part of the request.
"""

import csv
import os
from typing import Iterator

from .. import geometry, http, regions

RAW_BASE = "https://raw.githubusercontent.com/NationalGalleryOfArt/opendata/main/data"
FILES = (
    "objects.csv",
    "published_images.csv",
    "constituents.csv",
    "objects_constituents.csv",
    "objects_terms.csv",
)

HOST_GAPS = {"raw.githubusercontent.com": 1.0}

TERM_TYPES = {"Keyword", "Theme", "Style"}

csv.field_size_limit(10_000_000)  # a handful of provenance fields run long


def _collapse(text: str | None) -> str:
    return " ".join((text or "").split())


def _ensure_downloaded(client: http.PacedClient, raw_dir: str, refresh: bool) -> dict[str, str]:
    paths = {}
    for name in FILES:
        path = os.path.join(raw_dir, name)
        if refresh or not os.path.isfile(path):
            print(f"Downloading {name} ...")
            client.download(f"{RAW_BASE}/{name}", path)
        paths[name] = path
    return paths


def _byline(artist: str, date: str) -> str:
    artist, date = artist.strip(), date.strip()
    if artist and date:
        return f"{artist}, {date}"
    return artist or date


def fetch(
    client: http.PacedClient,
    cache_dir: str,
    refresh: bool,
    csv_path: str | None = None,  # unused; part of every source's shared signature
) -> Iterator[dict]:
    paths = _ensure_downloaded(client, os.path.join(cache_dir, "raw"), refresh)

    # 1. Candidates: paintings, with title and byline already in hand.
    candidates: dict[str, dict] = {}
    with open(paths["objects.csv"], encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            if row.get("classification") != "Painting":
                continue
            object_id = row.get("objectid")
            if not object_id:
                continue
            candidates[object_id] = {
                "title": _collapse(row.get("title")),
                "byline": _byline(row.get("attribution") or "", row.get("displaydate") or ""),
            }

    # 2. The primary open-access image and its real size, per candidate.
    images: dict[str, dict] = {}
    with open(paths["published_images.csv"], encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            object_id = row.get("depictstmsobjectid")
            if object_id not in candidates:
                continue
            if row.get("viewtype") != "primary" or row.get("openaccess") != "1":
                continue
            try:
                width, height = int(row["width"]), int(row["height"])
                sequence = int(row.get("sequence") or 0)
            except (KeyError, ValueError):
                continue
            existing = images.get(object_id)
            if existing is not None and existing["sequence"] <= sequence:
                continue
            images[object_id] = {
                "iiifurl": row["iiifurl"],
                "width": width,
                "height": height,
                "sequence": sequence,
            }

    # Only candidates with a usable image matter from here on; every
    # remaining pass over these large CSVs filters against this set so
    # memory stays proportional to the candidate pool, not the whole dataset.
    kept = images.keys() & candidates.keys()

    # 3. The primary artist's constituent id per candidate (lowest
    # displayorder wins — NGA's own convention for "the" credited artist).
    primary_artist: dict[str, str] = {}
    best_order: dict[str, int] = {}
    with open(paths["objects_constituents.csv"], encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            object_id = row.get("objectid")
            if object_id not in kept or row.get("roletype") != "artist":
                continue
            try:
                order = int(row.get("displayorder") or 0)
            except ValueError:
                order = 0
            if object_id not in best_order or order < best_order[object_id]:
                best_order[object_id] = order
                primary_artist[object_id] = row["constituentid"]

    # 4. Nationality for whichever constituents turned out to matter.
    needed_constituents = set(primary_artist.values())
    nationality: dict[str, str] = {}
    with open(paths["constituents.csv"], encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            cid = row.get("constituentid")
            if cid in needed_constituents:
                nationality[cid] = (row.get("nationality") or "").strip()

    # 5. Keyword/Theme/Style terms as tags.
    tags: dict[str, list[str]] = {}
    with open(paths["objects_terms.csv"], encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            object_id = row.get("objectid")
            if object_id not in kept:
                continue
            if row.get("termtype") not in TERM_TYPES:
                continue
            term = _collapse(row.get("term"))
            if term:
                tags.setdefault(object_id, []).append(term)

    for object_id in kept:
        image = images[object_id]
        width, height = geometry.fit_within(image["width"], image["height"])
        origin = nationality.get(primary_artist.get(object_id, ""), "")
        yield {
            "source": "nga",
            "id": object_id,
            "region": regions.region_from_text(origin),
            "width": width,
            "height": height,
            "image_url": f"{image['iiifurl']}/full/!4000,4000/0/default.jpg",
            "details_url": f"https://www.nga.gov/collection/art-object-page.{object_id}.html",
            "title": candidates[object_id]["title"] or "Untitled",
            "byline": candidates[object_id]["byline"],
            "origin": origin,
            "tags": tags.get(object_id, []),
        }
