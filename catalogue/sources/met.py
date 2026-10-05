"""The Metropolitan Museum of Art's open-access paintings.

Candidates come from the Met's own `MetObjects.csv` dump (public domain,
classification Paintings) — the same gate `android/catalogue/build.py` used
before this module replaced it. Everything else — the region, the size, the
title and byline, whether the image is even reachable — comes from a live
`GET .../objects/{id}` plus a small ranged `GET` of the image itself, because
the CSV snapshot can be stale and doesn't carry `country`/`culture` in a form
worth trusting over the live record.

Imperva, the Met's CDN, throttles a client to roughly 80 requests a minute;
`HOST_GAPS` below paces `collectionapi.metmuseum.org` well under that. A full
pass is on the order of three hours for a first run — every object's outcome
(row or skip, with why) is cached individually, so an interrupted run, or one
`SourceAborted` by a block, resumes rather than restarts.
"""

import csv
import os
from typing import Iterator

from .. import http, regions, text

API = "https://collectionapi.metmuseum.org/public/collection/v1"
CSV_URL = "https://media.githubusercontent.com/media/metmuseum/openaccess/master/MetObjects.csv"
DEFAULT_CSV_PATH = os.path.expanduser("~/.cache/art-window/MetObjects.csv")

# Two hosts, two very different tolerances: collectionapi is what Imperva
# watches, images.metmuseum.org is plain image hosting.
HOST_GAPS = {
    "collectionapi.metmuseum.org": 2.0,
    "images.metmuseum.org": 0.5,
}

# A ranged GET first asks for this many bytes — enough for the SOF marker on
# every real-world JPEG this source has seen — and, only if that comes back
# without one, asks once more for this larger range before giving up on the
# object's dimensions entirely.
RANGE_INITIAL = 128 * 1024
RANGE_FALLBACK = 1024 * 1024

_SOF_MARKERS = frozenset(
    {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}
)


def parse_jpeg_size(data: bytes) -> tuple[int, int] | None:
    """The pixel size out of a JPEG's SOF marker, read from `data` — which
    need not be the whole file. The SOF marker sits near the start, well
    before the (potentially huge) entropy-coded scan data, which is the
    whole point: a painting's real size can be read from a few KB fetched
    with a `Range` header, at a fraction of a full download.

    `None` means no SOF marker was found in `data` — not that the image is
    invalid, just that the caller fetched too small a prefix and should try
    a larger one.
    """
    if len(data) < 4 or data[0:2] != b"\xff\xd8":
        return None
    i = 2
    n = len(data)
    while i + 4 <= n:
        if data[i] != 0xFF:
            i += 1
            continue
        marker = data[i + 1]
        i += 2
        if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
            continue  # markers with no length/payload
        if marker == 0xD9:  # EOI, reached before any SOF
            return None
        if i + 2 > n:
            return None
        length = (data[i] << 8) | data[i + 1]
        if marker in _SOF_MARKERS:
            if i + 7 > n:
                return None  # SOF found but truncated before height/width
            height = (data[i + 3] << 8) | data[i + 4]
            width = (data[i + 5] << 8) | data[i + 6]
            return width, height
        i += length  # `length` includes its own two bytes, so this lands on the next marker
    return None


def _download_csv(client: http.PacedClient, csv_path: str) -> None:
    print(f"Downloading MetObjects.csv to {csv_path} ...")
    client.download(CSV_URL, csv_path)


def _iter_candidates(csv_path: str) -> Iterator[tuple[int, str]]:
    """`(object id, maker)` per public-domain painting. The maker comes from
    the CSV rather than the per-object cache, whose format a three-hour pass
    depends on staying put."""
    with open(csv_path, encoding="utf-8-sig", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            if row.get("Is Public Domain") != "True":
                continue
            if row.get("Classification") != "Paintings":
                continue
            try:
                yield int(row["Object ID"]), text.maker(row.get("Artist Display Name") or "")
            except (KeyError, ValueError):
                continue


def _read_dimensions(client: http.PacedClient, image_url: str) -> tuple[int, int] | None:
    for end in (RANGE_INITIAL - 1, RANGE_FALLBACK - 1):
        data = client.get_range(image_url, 0, end)
        dims = parse_jpeg_size(data)
        if dims is not None:
            return dims
        if len(data) < end + 1:
            # The server sent less than we asked for — either the whole file
            # (Range ignored) or a file smaller than the range — so a bigger
            # ask won't turn up more bytes.
            break
    return None


def _fetch_one(client: http.PacedClient, object_id: int) -> dict:
    """One object's outcome: either a row's worth of fields, or `{"skip":
    reason}`. Never raises for an ordinary "this object doesn't work out" —
    only `http.SourceAborted` escapes, when the host itself has stopped
    answering."""
    obj = client.get_json(f"{API}/objects/{object_id}")

    image_url = (obj.get("primaryImage") or "").strip()
    if not image_url:
        return {"skip": "no primaryImage"}
    if not image_url.lower().endswith((".jpg", ".jpeg")):
        return {"skip": f"primaryImage is not a JPEG: {image_url}"}

    dims = _read_dimensions(client, image_url)
    if dims is None:
        return {"skip": "could not read JPEG dimensions from either range"}
    width, height = dims

    region = (
        regions.met_department_region(obj.get("department"))
        or regions.region_from_text(obj.get("country"))
        or regions.region_from_text(obj.get("culture"))
        or regions.region_from_text(obj.get("artistNationality"))
    )

    tags = [t["term"] for t in (obj.get("tags") or []) if t.get("term")]

    return {
        "title": (obj.get("title") or "").strip(),
        "byline": text.byline(obj.get("artistDisplayName") or "", obj.get("objectDate") or ""),
        "origin": (obj.get("country") or obj.get("culture") or "").strip(),
        "region": region,
        "width": width,
        "height": height,
        "image_url": image_url,
        "details_url": obj.get("objectURL") or "",
        "tags": tags,
    }


def fetch(
    client: http.PacedClient,
    cache_dir: str,
    refresh: bool,
    csv_path: str | None = None,
) -> Iterator[dict]:
    csv_path = csv_path or DEFAULT_CSV_PATH
    if not os.path.isfile(csv_path):
        _download_csv(client, csv_path)

    cache = http.JsonCache(os.path.join(cache_dir, "objects"))
    for object_id, maker in _iter_candidates(csv_path):
        key = str(object_id)
        record = None if refresh else cache.get(key)
        if record is None:
            try:
                record = _fetch_one(client, object_id)
            except http.SourceAborted:
                raise
            except Exception as e:  # noqa: BLE001 — one bad object must not sink the pass
                record = {"skip": f"error: {e}"}
            cache.put(key, record)

        if record.get("skip"):
            continue

        yield {
            "source": "met",
            "id": key,
            "region": record["region"],
            "width": record["width"],
            "height": record["height"],
            "image_url": record["image_url"],
            "details_url": record["details_url"],
            "title": record["title"] or "Untitled",
            "byline": record["byline"],
            "origin": record["origin"],
            "tags": record["tags"],
            "artist": maker,
        }
