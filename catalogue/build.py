#!/usr/bin/env python3
"""Builds `catalogue/dist/paintings.tsv` — the prebuilt list of museum
paintings both the Android app and the desktop ship inside their bundles, so
neither has to search a live API on someone's phone or laptop.

Each of the five sources under `sources/` applies only *objective* gates:
public domain or CC0, classified as a painting, a direct JPEG, and (via
`geometry.meets_minimum`, applied once below rather than by each source) a
long side of at least `geometry.MIN_LONG_SIDE` pixels. Subject, portrait,
religious-scene and shape judgments stay out of this file entirely — they
live in `Catalogue.kt` on Android and `museums.rs` on the desktop, the two
places that actually decide what a person sees.

Four sources are museums; the fifth, `wmc` (`sources/commons.py`), instead
walks Wikimedia Commons categories for one named artist at a time. Its rows
carry an `artist` display name in the 12th column, empty for every museum
row. To add an artist, add one entry to `catalogue/artists.json`:
`{"name": ..., "region": <one of catalogue.regions.REGIONS>, "categories":
[...]}`, where `categories` are Commons category names (without the
"Category:" prefix) walked recursively for that artist's paintings.

Usage:
    python3 catalogue/build.py [--only met,nga,cma,smk,wmc] [--refresh] [--csv PATH]

Everything each source downloads is cached under
`~/.cache/art-window/catalogue/<source>/`, so an interrupted run — or the
Met's multi-hour first pass — resumes rather than starts over. `--refresh`
ignores that cache and re-fetches everything for the sources being run.

`--only` narrows which sources make new requests; it does not shrink the
file. A source left out this run has its rows read back from the existing
`dist/paintings.tsv` and kept as-is, so `--only wmc` after a museum-only
build adds artist rows without discarding the museums', and any one source
can be rerun on its own without waiting on — or erasing — the other four.
"""

import sys
import os

# Python auto-adds this script's own directory to `sys.path[0]`. A file
# `catalogue/http.py` sitting directly in that directory would then shadow
# the stdlib `http` package for the rest of the process — `urllib.request`
# imports `http.client`, and finds this one first. So: drop that entry and
# import everything through the `catalogue` package from the repo root
# instead, before anything below has a chance to import `urllib`.
_here = os.path.dirname(os.path.realpath(__file__))
sys.path = [p for p in sys.path if os.path.realpath(p) != _here]
_repo_root = os.path.dirname(_here)
if _repo_root not in sys.path:
    sys.path.insert(0, _repo_root)

import argparse  # noqa: E402
import datetime  # noqa: E402
import re  # noqa: E402

from catalogue import geometry, http, regions  # noqa: E402
from catalogue.sources import cleveland, commons, met, nga, smk  # noqa: E402

SOURCE_MODULES = {"met": met, "nga": nga, "cma": cleveland, "smk": smk, "wmc": commons}
SOURCE_ORDER = ("met", "nga", "cma", "smk", "wmc")

TEXT_FIELDS = ("image_url", "details_url", "title", "byline", "origin")

_SNAPSHOT_RE = re.compile(r"(\w+) \((\d{4}-\d{2}-\d{2})\)")


def _collapse(text: str | None) -> str:
    return " ".join((text or "").split())


def _read_existing_snapshot_dates(dist_path: str) -> dict[str, str]:
    """The `source (date)` pairs out of a previously written file's header
    comment, so a run that leaves a source out of `--only` keeps reporting
    the date its rows actually came from, rather than relabelling them as
    today's."""
    if not os.path.isfile(dist_path):
        return {}
    with open(dist_path, encoding="utf-8") as f:
        first_line = f.readline()
    return dict(_SNAPSHOT_RE.findall(first_line))


def _read_existing_rows(dist_path: str, keep_sources: set[str]) -> list[dict]:
    """Rows already on disk whose `source` is in `keep_sources` — the
    sources this run did *not* select — so `--only` can narrow which sources
    make new requests without shrinking the file down to just those. Reads
    both the legacy 11-column shape and the current 12-column one."""
    rows: list[dict] = []
    if not os.path.isfile(dist_path):
        return rows
    with open(dist_path, encoding="utf-8") as f:
        for line in f:
            if not line.strip() or line.startswith("#"):
                continue
            fields = line.rstrip("\n").split("\t")
            if len(fields) < 11 or fields[0] not in keep_sources:
                continue
            rows.append({
                "source": fields[0],
                "id": fields[1],
                "region": fields[2],
                "width": int(fields[3]),
                "height": int(fields[4]),
                "image_url": fields[5],
                "details_url": fields[6],
                "title": fields[7],
                "byline": fields[8],
                "origin": fields[9],
                "tags": fields[10].split("|") if fields[10] else [],
                "artist": fields[11] if len(fields) > 11 else "",
            })
    return rows


def _write(rows: list[dict], dist_path: str, snapshot_dates: dict[str, str]) -> None:
    os.makedirs(os.path.dirname(dist_path), exist_ok=True)
    sources_note = ", ".join(
        f"{name} ({snapshot_dates[name]})" for name in SOURCE_ORDER if name in snapshot_dates
    )
    with open(dist_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(f"# Generated by catalogue/build.py from {sources_note}. Do not edit by hand.\n")
        for row in rows:
            tags = "|".join(_collapse(t) for t in row["tags"] if _collapse(t))
            fields = [
                row["source"],
                row["id"],
                row["region"],
                str(row["width"]),
                str(row["height"]),
                *(_collapse(row[field]) for field in TEXT_FIELDS),
                tags,
                _collapse(row.get("artist", "")),
            ]
            f.write("\t".join(fields) + "\n")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--only", default=",".join(SOURCE_ORDER), help="comma-separated subset of met,nga,cma,smk,wmc"
    )
    parser.add_argument(
        "--refresh", action="store_true", help="ignore each selected source's cache and re-fetch everything"
    )
    parser.add_argument(
        "--csv", dest="csv_path", default=None,
        help="local MetObjects.csv path (met only; downloaded to ~/.cache/art-window/MetObjects.csv if omitted)",
    )
    args = parser.parse_args(argv)

    requested = {s.strip() for s in args.only.split(",") if s.strip()}
    unknown = requested - set(SOURCE_MODULES)
    if unknown:
        parser.error(f"unknown source(s): {', '.join(sorted(unknown))} (choose from {', '.join(SOURCE_ORDER)})")

    dist_path = os.path.join(_repo_root, "catalogue", "dist", "paintings.tsv")

    today = datetime.date.today().isoformat()
    rows: list[dict] = []
    snapshot_dates = _read_existing_snapshot_dates(dist_path)

    for name in SOURCE_ORDER:
        if name not in requested:
            continue
        module = SOURCE_MODULES[name]
        print(f"=== {name} ===", flush=True)
        client = http.PacedClient(gap_seconds=module.HOST_GAPS)
        cache_dir = os.path.expanduser(f"~/.cache/art-window/catalogue/{name}")
        before = len(rows)
        try:
            for row in module.fetch(client, cache_dir, args.refresh, args.csv_path):
                rows.append(row)
        except http.SourceAborted as e:
            print(f"warning: {name} aborted: {e}", file=sys.stderr)
        snapshot_dates[name] = today
        print(f"{name}: {len(rows) - before} candidate rows this run", flush=True)

    accepted = []
    dropped_region = dropped_size = 0
    for row in rows:
        if row["region"] not in regions.REGIONS:
            dropped_region += 1
            continue
        if not geometry.meets_minimum(row["width"], row["height"]):
            dropped_size += 1
            continue
        accepted.append(row)

    kept = _read_existing_rows(dist_path, keep_sources=set(SOURCE_MODULES) - requested)
    combined = accepted + kept
    combined.sort(key=lambda r: (r["source"], r["id"]))
    _write(combined, dist_path, snapshot_dates)

    print(f"\nWrote {len(combined)} rows to {dist_path} "
          f"({len(accepted)} freshly fetched, {len(kept)} kept from disk)")
    print(f"Dropped: {dropped_region} unknown region, {dropped_size} under {geometry.MIN_LONG_SIDE}px long side")
    counts: dict[tuple[str, str], int] = {}
    for row in combined:
        key = (row["source"], row["region"])
        counts[key] = counts.get(key, 0) + 1
    for (source, region), count in sorted(counts.items()):
        print(f"  {source:4s} {region:14s} {count}")


if __name__ == "__main__":
    main()
