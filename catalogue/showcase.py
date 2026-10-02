"""The artist browser's data: one bundled picture and one "about" link per
named painter, written to `catalogue/dist/artists/`.

For every painter whose `artist` column survived `build.MIN_PER_CHOICE`, this
writes `<slug>.jpg` (that painter's `showcase` painting from `artists.json`, at
most `MAX_LONG_SIDE` pixels on the long side) and `index.tsv`, whose columns are
`name region about showcase title byline file`. `name`, `title` and `byline`
are copied from the painter's `showcase` row in the combined rows, so they
agree byte for byte with `paintings.tsv`. A picture whose painter no longer has
a row is deleted. A surviving painter with no `about`, no `showcase`, or a
`showcase` that is not one of their own rows fails the run: nothing here guesses.

The pictures go through the Commons source's own paced, cached request path
and are downscaled here with Pillow (Commons only serves thumbnails at fixed
widths); an existing file is kept unless `refresh` is set.
"""

import os
import re
import unicodedata

from . import http
from .sources import commons

MAX_LONG_SIDE = 1400

INDEX_COLUMNS = ("name", "region", "about", "showcase", "title", "byline", "file")


def slug(name: str) -> str:
    """`Almeida Júnior` -> `almeida-junior`."""
    folded = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", folded.lower()).strip("-")


def _entries(rows: list[dict]) -> list[tuple[dict, dict]]:
    """`(artists.json entry, showcase row)` for each painter still named in
    `rows`, sorted by name. Raises `SystemExit` naming the first one that
    cannot be served."""
    by_id = {row["id"]: row for row in rows if row["source"] == "wmc"}
    named = {row["artist"] for row in rows if row.get("artist")}
    entries = []
    for artist in commons._load_artists():
        name = artist["name"]
        if name not in named:
            continue
        if not artist.get("about"):
            raise SystemExit(f"showcase: {name} keeps an Artist chip but has no \"about\" in artists.json")
        if not artist.get("showcase"):
            raise SystemExit(f"showcase: {name} keeps an Artist chip but has no \"showcase\" in artists.json")
        row = by_id.get(str(artist["showcase"]))
        if row is None or row.get("artist") != name:
            raise SystemExit(
                f"showcase: {name}'s showcase {artist['showcase']!r} is not one of their wmc rows in paintings.tsv"
            )
        entries.append((artist, row))
    return sorted(entries, key=lambda e: e[0]["name"])


def _source_url(client: http.PacedClient, cache: http.JsonCache, refresh: bool, page_id: str) -> str:
    """A URL for Commons page `page_id` that is at least `MAX_LONG_SIDE` on its
    long side where the original is. Commons only serves thumbnails at its
    standard widths (1280, 1920, ...), so a 1400 px request would be rounded up
    anyway; `write` does the final downscale itself."""
    data = commons._cached_get_json(
        client, cache, refresh,
        f"{commons.API}?" + commons.urlencode({
            "action": "query", "format": "json", "pageids": page_id, "prop": "imageinfo",
            "iiprop": "url|size", "iiurlwidth": "1920", "maxlag": "5",
        }),
    )
    info = list(data["query"]["pages"].values())[0]["imageinfo"][0]
    return info.get("thumburl") or info["url"]


def _fetch(client: http.PacedClient, cache: http.JsonCache, refresh: bool, page_id: str, dest: str) -> None:
    try:
        from PIL import Image
    except ImportError:
        raise SystemExit("showcase: Pillow is needed to downscale the pictures (pip3 install pillow)")
    raw = dest + ".download"
    client.download(_source_url(client, cache, refresh, page_id), raw)
    try:
        with Image.open(raw) as image:
            image = image.convert("RGB")
            image.thumbnail((MAX_LONG_SIDE, MAX_LONG_SIDE), Image.LANCZOS)
            image.save(dest, "JPEG", quality=88, optimize=True)
    finally:
        os.remove(raw)


def write(rows: list[dict], out_dir: str, refresh: bool) -> None:
    entries = _entries(rows)
    os.makedirs(out_dir, exist_ok=True)
    client = http.PacedClient(gap_seconds=commons.HOST_GAPS)
    cache = http.JsonCache(os.path.expanduser("~/.cache/art-window/catalogue/wmc/api"))

    wanted = set()
    index_lines = []
    for artist, row in entries:
        file_name = f"{slug(artist['name'])}.jpg"
        wanted.add(file_name)
        dest = os.path.join(out_dir, file_name)
        if refresh or not os.path.isfile(dest):
            print(f"  showcase: {artist['name']} -> {file_name}", flush=True)
            _fetch(client, cache, refresh, str(artist["showcase"]), dest)
        fields = [artist["name"], artist["region"], artist["about"], str(artist["showcase"]),
                  row["title"], row["byline"], file_name]
        index_lines.append("\t".join(" ".join(f.split()) for f in fields))

    with open(os.path.join(out_dir, "index.tsv"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# Generated by catalogue/build.py (showcase.py) from catalogue/artists.json. Do not edit by hand.\n")
        f.write("\n".join(index_lines) + "\n")
    for existing in os.listdir(out_dir):
        if existing.endswith(".jpg") and existing not in wanted:
            os.remove(os.path.join(out_dir, existing))
