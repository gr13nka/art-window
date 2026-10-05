#!/usr/bin/env python3
"""Builds `catalogue/dist/paintings.tsv` — the prebuilt list of museum
paintings the desktop, Android and iOS apps all ship inside their bundles, so
none has to search a live API on someone's phone or laptop.

Each of the sources under `sources/` applies only *objective* gates:
public domain or CC0, classified as a painting, a direct JPEG, and (via
`geometry.meets_minimum`, applied once below rather than by each source) a
long side of at least `geometry.MIN_LONG_SIDE` pixels. Subject, portrait,
religious-scene and shape judgments stay out of this file entirely — they
live in `Catalogue.kt` on Android, `museums.rs` on the desktop and
`Catalogue.swift` on iOS, the three places that actually decide what a
person sees.

Every source is a museum that itself grants commercial use of its public-domain
images. Each row also carries the maker the museum names, which `text.maker`
reduces to a plain personal name or nothing; `_apply_curated_list` then keeps
that name in the 12th `artist` column only for painters listed in
`catalogue/artists.json`. To add a painter, run the build, read the report's
"Makers ... not on the list" section for the spellings the museums use, and add
one entry: `{"name": <chip name>, "aliases": [<raw maker strings>], "about":
<URL>, "showcase": "<source>:<id>"}`, the showcase being one of that painter's
own rows.

An artist with fewer than `MIN_PER_CHOICE` rows in the finished file has its
`artist` column blanked (the rows stay; the byline still names the painter),
because that column is what puts an *Artist* chip in every app's settings. The
run ends with a report of thin spots: rows per (source, region), per region
(all six, zeros included), per named artist, the artists just blanked, the makers the museums name that the
list does not, and —
when `cargo` is on the PATH — the desktop app's own region x subject x shape
counts from `cargo run -- --catalogue`.

Every painter whose artist column survives that rule also gets an entry in
`catalogue/dist/artists/` (written by `showcase.py` after the TSV, from the combined
rows, so it runs under any `--only`): `<slug>.jpg`, the painter's `showcase`
painting from `artists.json` (downloaded from that row's own image URL) at no more than 1400 px, and `index.tsv`
(`name region about showcase title byline file`, sorted by name; `name`,
`title` and `byline` are copied from the TSV so they match it exactly). The
desktop's artist browser compiles these in; Android bundles them with the rest
of `dist/` as assets, and iOS as a folder resource. A painter who keeps the
chip but lacks `about`, `showcase`, or a showcase that is one of their own rows
fails the build.

Usage:
    python3 catalogue/build.py [--only met,nga,cma,smk] [--refresh] [--csv PATH]
                               [--no-app-report]

Everything each source downloads is cached under
`~/.cache/art-window/catalogue/<source>/`, so an interrupted run — or the
Met's multi-hour first pass — resumes rather than starts over. `--refresh`
ignores that cache and re-fetches everything for the sources being run.

`--only` narrows which sources make new requests; it does not shrink the
file. A source left out this run has its rows read back from the existing
`dist/paintings.tsv` and kept as-is, so any one source can be rerun on its
own without waiting on — or erasing — the others. What a kept row loses is its raw
maker, which `paintings.tsv` no longer holds once the list has mapped it; that
survives in `catalogue/makers.tsv` (see `_read_makers`).

A source that aborts part-way — a host that kept refusing — is treated the
same way: what it managed this run is thrown away, its rows already on disk
are kept under the date they were fetched, and the run exits non-zero after
writing the file. Its cache has the progress, so running it again resumes.
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
import shutil  # noqa: E402
import subprocess  # noqa: E402

from catalogue import geometry, http, regions, showcase, text  # noqa: E402
from catalogue.sources import cleveland, getty, met, nga, rijksmuseum, smk  # noqa: E402

SOURCE_MODULES = {"met": met, "nga": nga, "cma": cleveland, "smk": smk, "rijks": rijksmuseum, "getty": getty}
SOURCE_ORDER = ("met", "nga", "cma", "smk", "rijks", "getty")

# Mirrors `MIN_POOL` in `src/art/museums.rs` on purpose, the same way the word
# lists are duplicated across platforms: a filter choice with fewer paintings
# than this is not worth offering.
MIN_PER_CHOICE = 20

TEXT_FIELDS = ("image_url", "details_url", "title", "byline", "origin")

_SNAPSHOT_RE = re.compile(r"(\w+) \((\d{4}-\d{2}-\d{2})\)")


def _read_existing_snapshot_dates(dist_path: str) -> dict[str, str]:
    """The `source (date)` pairs out of a previously written file's header
    comment, so a run that leaves a source out of `--only` keeps reporting
    the date its rows actually came from, rather than relabelling them as
    today's."""
    if not os.path.isfile(dist_path):
        return {}
    with open(dist_path, encoding="utf-8") as f:
        first_line = f.readline()
    return {name: date for name, date in _SNAPSHOT_RE.findall(first_line) if name in SOURCE_ORDER}


def _read_existing_rows(dist_path: str, keep_sources: set[str], makers: dict[tuple[str, str], str]) -> list[dict]:
    """Rows already on disk whose `source` is in `keep_sources` — the
    sources this run did *not* select — so `--only` can narrow which sources
    make new requests without shrinking the file down to just those. Each row's
    `artist` is its raw maker from `makers`, not the 12th column, which holds
    only what the list let through last time."""
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
                "artist": makers.get((fields[0], fields[1]), ""),
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
            tags = "|".join(text.collapse(t) for t in row["tags"] if text.collapse(t))
            fields = [
                row["source"],
                row["id"],
                row["region"],
                str(row["width"]),
                str(row["height"]),
                *(text.collapse(row[field]) for field in TEXT_FIELDS),
                tags,
                text.collapse(row.get("artist", "")),
            ]
            f.write("\t".join(fields) + "\n")


def _read_makers(path: str) -> dict[tuple[str, str], str]:
    """`{(source, id): raw maker}` from `catalogue/makers.tsv`.

    A partial run keeps some sources' rows from disk, but `paintings.tsv` holds
    only the *mapped* artist, and mapping is lossy: a painter added to the list
    later could never be found among rows already blanked. Re-fetching every
    source to recover the maker is exactly what `--only` exists to avoid, so the
    build writes each row's raw maker beside the catalogue and reads it back. It
    sits in `catalogue/`, not `dist/`, because Android bundles all of `dist/` and
    no app has any use for it. Absent on the first run, in which case kept rows
    have no maker until their source is run once."""
    makers: dict[tuple[str, str], str] = {}
    if os.path.isfile(path):
        with open(path, encoding="utf-8") as f:
            for line in f:
                if line.startswith("#") or not line.strip():
                    continue
                fields = line.rstrip("\n").split("\t")
                if len(fields) == 3:
                    makers[(fields[0], fields[1])] = fields[2]
    return makers


def _write_makers(rows: list[dict], path: str) -> None:
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("# Generated by catalogue/build.py: each painting's maker as its museum names it. Do not edit by hand.\n")
        for row in rows:
            if row.get("maker"):
                f.write(f"{row['source']}\t{row['id']}\t{text.collapse(row['maker'])}\n")


def _apply_curated_list(rows: list[dict]) -> dict[str, dict[str, int]]:
    """Sets every row's `artist` to the listed painter its raw `maker` stands
    for, or to nothing, and returns `{maker: {source: rows}}` for the makers
    that matched no one.

    The list is curated because museums name hundreds of makers nobody would
    pick from a settings screen, and spell one painter several ways ("Claude
    Monet", "Monet, Claude", "Claude-Oscar Monet"); a painter exists for the apps
    only once someone has listed the spellings that mean them. Names are compared
    after `text.collapse` and case-folding."""
    wanted: dict[str, str] = {}
    for painter in showcase.load_artists():
        for spelling in (painter["name"], *painter.get("aliases", [])):
            wanted[text.collapse(spelling).casefold()] = painter["name"]
    unlisted: dict[str, dict[str, int]] = {}
    for row in rows:
        maker = text.collapse(row.get("maker"))
        row["artist"] = wanted.get(maker.casefold(), "")
        if maker and not row["artist"]:
            per_source = unlisted.setdefault(maker, {})
            per_source[row["source"]] = per_source.get(row["source"], 0) + 1
    return unlisted


def _blank_thin_artists(rows: list[dict]) -> dict[str, int]:
    """Blanks the `artist` column of every row whose artist has fewer than
    `MIN_PER_CHOICE` rows in `rows`, and returns `{artist: count}` for the
    artists it blanked. The artist column is what puts an *Artist* chip in
    every app's settings, and a chip that leads to three paintings is the bug
    `MIN_PER_CHOICE` exists to prevent. `rows` must be the combined set — fresh
    plus kept from disk — so an artist's count is the file's, not this run's."""
    counts: dict[str, int] = {}
    for row in rows:
        if row.get("artist"):
            counts[row["artist"]] = counts.get(row["artist"], 0) + 1
    thin = {name: n for name, n in counts.items() if n < MIN_PER_CHOICE}
    for row in rows:
        if row.get("artist") in thin:
            row["artist"] = ""
    return thin


def _print_report(
    combined: list[dict], blanked: dict[str, int], unlisted: dict[str, dict[str, int]], app_report: bool
) -> None:
    by_source_region: dict[tuple[str, str], int] = {}
    by_region = dict.fromkeys(regions.REGIONS, 0)
    by_artist: dict[str, int] = {}
    for row in combined:
        key = (row["source"], row["region"])
        by_source_region[key] = by_source_region.get(key, 0) + 1
        by_region[row["region"]] += 1
        if row.get("artist"):
            by_artist[row["artist"]] = by_artist.get(row["artist"], 0) + 1

    def flag(count: int) -> str:
        return f"  <-- under {MIN_PER_CHOICE}" if count < MIN_PER_CHOICE else ""

    for (source, region), count in sorted(by_source_region.items()):
        print(f"  {source:4s} {region:14s} {count}")
    print("\nPer region:")
    for region in regions.REGIONS:
        print(f"  {region:14s} {by_region[region]}{flag(by_region[region])}")
    print("\nPer named artist:")
    for name, count in sorted(by_artist.items()):
        print(f"  {name:32s} {count}{flag(count)}")
    if blanked:
        print(f"\nArtist column blanked (under {MIN_PER_CHOICE} rows):")
        for name, count in sorted(blanked.items()):
            print(f"  {name:32s} {count}")

    candidates = sorted(
        ((sum(per_source.values()), maker, per_source) for maker, per_source in unlisted.items()),
        key=lambda c: (-c[0], c[1]),
    )
    candidates = [c for c in candidates if c[0] >= MIN_PER_CHOICE]
    print(f"\nMakers with {MIN_PER_CHOICE}+ rows not on the list (catalogue/artists.json):")
    for count, maker, per_source in candidates:
        sources = ", ".join(f"{s} {n}" for s, n in sorted(per_source.items()))
        print(f"  {maker:36s} {count:5d}  ({sources})")

    command = "cargo run --quiet -- --catalogue"
    if not app_report:
        return
    if shutil.which("cargo") is None:
        print(f"\nApp-side report: cargo not found; run `{command}` from the repo root.")
        return
    print("\nApp-side report (what the desktop app sees):", flush=True)
    result = subprocess.run(command.split(), cwd=_repo_root, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"App-side report unavailable (cargo exited {result.returncode}); run `{command}` by hand.")
    else:
        print(result.stdout, end="")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--only", default=",".join(SOURCE_ORDER), help="comma-separated subset of " + ",".join(SOURCE_ORDER)
    )
    parser.add_argument(
        "--refresh", action="store_true", help="ignore each selected source's cache and re-fetch everything"
    )
    parser.add_argument(
        "--csv", dest="csv_path", default=None,
        help="local MetObjects.csv path (met only; downloaded to ~/.cache/art-window/MetObjects.csv if omitted)",
    )
    parser.add_argument(
        "--no-app-report", action="store_true",
        help="skip the closing `cargo run -- --catalogue` report of the desktop app's own counts",
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
    aborted: list[str] = []

    for name in SOURCE_ORDER:
        if name not in requested:
            continue
        module = SOURCE_MODULES[name]
        print(f"=== {name} ===", flush=True)
        client = http.PacedClient(gap_seconds=module.HOST_GAPS)
        cache_dir = os.path.expanduser(f"~/.cache/art-window/catalogue/{name}")
        fresh: list[dict] = []
        try:
            for row in module.fetch(client, cache_dir, args.refresh, args.csv_path):
                fresh.append(row)
        except http.SourceAborted as e:
            # A pass cut short holds a fragment of its source. Written out, the
            # fragment would replace that source's whole set in a file every app
            # compiles in, under today's date — so it is dropped, and the rows on
            # disk stand in for it exactly as they do for a source not requested.
            print(
                f"warning: {name} aborted after {len(fresh)} rows: {e}\n"
                f"         keeping the {name} rows already on disk; run it again to resume",
                file=sys.stderr,
            )
            aborted.append(name)
            continue
        rows.extend(fresh)
        snapshot_dates[name] = today
        print(f"{name}: {len(fresh)} candidate rows this run", flush=True)

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

    makers_path = os.path.join(_repo_root, "catalogue", "makers.tsv")
    for row in accepted:
        row["maker"] = row.get("artist", "")
    kept = _read_existing_rows(
        dist_path,
        keep_sources=(set(SOURCE_MODULES) - requested) | set(aborted),
        makers=_read_makers(makers_path),
    )
    for row in kept:
        row["maker"] = row.get("artist", "")
    combined = accepted + kept
    unlisted = _apply_curated_list(combined)
    blanked = _blank_thin_artists(combined)
    combined.sort(key=lambda r: (r["source"], r["id"]))
    _write(combined, dist_path, snapshot_dates)
    _write_makers(combined, makers_path)
    showcase.write(combined, os.path.join(_repo_root, "catalogue", "dist", "artists"), args.refresh,
                   host_gaps={h: g for m in SOURCE_MODULES.values() for h, g in m.HOST_GAPS.items()})

    print(f"\nWrote {len(combined)} rows to {dist_path} "
          f"({len(accepted)} freshly fetched, {len(kept)} kept from disk)")
    print(f"Dropped: {dropped_region} unknown region, {dropped_size} under {geometry.MIN_LONG_SIDE}px long side")
    _print_report(combined, blanked, unlisted, app_report=not args.no_app_report)
    if aborted:
        sys.exit(f"\nIncomplete: {', '.join(aborted)} aborted and kept its rows from disk.")


if __name__ == "__main__":
    main()
