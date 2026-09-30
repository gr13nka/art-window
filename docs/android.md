# Art Window for Android

The desktop app fits a painting inside the screen and letterboxes the rest in
black. A phone screen is the wrong shape for that trick, so the Android app makes
different choices at almost every step. This is a record of those choices and why
each one is different from the desktop's.

## Why Kotlin, not Rust

The desktop app is mostly platform glue that has no Android equivalent: a tray
icon and menu, an event loop that waits on wake notifications and a ticking
clock, a `config.toml` a human edits, and a login item. None of that exists on
Android, which has its own scheduler, its own wallpaper API and no menu bar to
hang anything off.

What *could* be shared is smaller than it looks, and most of it already is
one thing: `catalogue/build.py`, a single Python pipeline that walks all four
museums and writes `catalogue/dist/paintings.tsv` — the desktop compiles that
file in, Android bundles it as an asset, and neither platform repeats any of
the network logic that built it. What's left is judgment a build script
can't make once and hand over: which catalogue entry is a landscape, a
portrait or a religious scene, and what shape a screen wants. Sharing *that*
would mean putting Kotlin logic behind `cargo-ndk`, building for four ABIs,
and writing a JNI bridge to carry it across — machinery bigger than the couple
of hundred lines of rules it would share, for a platform that already has
`HttpURLConnection` and `org.json` built in.

The cost of not sharing it is that those rules live twice: once in
`src/art/museums.rs`, once in `android/app/src/main/java/dev/artwindow/Catalogue.kt`.
A change to a subject's word list — including the Danish ones SMK's
Danish-language records need, see **Subject choice and the religious filter**
below — or to the portrait exclusion belongs in both files, and each carries a
comment pointing at the other so that isn't easy to forget. What Android's
`Museums.kt` (renamed from `Met.kt` along with the rest of the app, now that a
turn can draw from any of the four museums) duplicates is much smaller: the
`{source}-{id}.{ext}` filename convention a download has to carry so `keyOf`
can recognise this module's own work later — the same reasoning as the
desktop's `museums-` prefix (see `CLAUDE.md`'s Android invariants).

## Artwork shape is a preference on Android

The desktop's **No filtering by the shape of a picture** rule (see the main
`CLAUDE.md`) holds because fit-plus-letterbox renders any painting well. A phone
screen is roughly 0.45 units wide per unit of height, close to nothing in any of
the four museums' collections, so the Android default remains **Phone-shaped** (`ArtworkShape.SCREEN`, labelled TV-shaped on a television): paintings that
Zoom can fill without grotesque cropping. Settings can broaden that pool to
include near-square work (up to a 1.25 width/height ratio) or any shape, including
fully horizontal work. This artwork-shape choice is independent of the rendering
style.

## The local catalogue

A turn no longer touches a museum's live API to choose a painting. The pool is
`catalogue/dist/paintings.tsv`, a UTF-8 TSV built offline at the repo root by
`catalogue/build.py` (Python 3, stdlib only) — a pipeline shared with the
desktop, which compiles the same file straight in. It draws from four
museums now, not the Met alone: the Met, the National Gallery of Art
(Washington), the Cleveland Museum of Art and SMK (Denmark). It is checked in
and never hand-edited; regenerate it with `python3 catalogue/build.py` (see
`CLAUDE.md`'s Commands and External services sections for pacing, licensing
and how long a Met pass takes — about three hours, resumable). The Gradle
build bundles it straight from `catalogue/dist/` as an asset
(`sourceSets["main"].assets.srcDir("../../catalogue/dist")` in
`build.gradle.kts`), so there's no copy inside `android/` to fall out of sync.

The build applies only **objective** gates: public domain or CC0, catalogued
as a painting, has a direct JPEG, is at least 2000 px on its long side, and
names a region the shared `regions.py` recognises. Crucially, the width and
height in each row are the photograph's *real, verified* pixel size — parsed
from the JPEG itself at build time, never trusted from a museum's own
catalogue record — which is what lets `Catalogue.candidates` decide shape and
renderability outright. There is no live measurement, no web-sized preview
and no "does this actually hold up" step left to run, because the numbers are
already known to be correct. Subject, portrait, religious-content and shape
decisions all stay out of the build and in the apps, same as before —
`Catalogue.kt` is still the one place on Android that makes them (see
**Subject choice and the religious filter** below).

Because every candidate's pixels are already verified, a turn spends exactly
one HTTP request in the ordinary case: `Museums.fetch` shuffles the entries
`Catalogue.candidates` returns, downloads the first one's `imageUrl`, and
checks that the decoded size is within 2% of what the catalogue promised
(`TOLERANCE`). A mismatch — a museum having re-encoded an image between the
catalogue's build and this download — deletes the file and moves to the next
candidate, up to `MAX_ATTEMPTS = 3`; when nothing in `MAX_ATTEMPTS` downloads
holds up, the error names how many were tried. If the catalogue has nothing
at all matching the chosen region, subject and shape settings, `fetch` fails
fast with "No paintings in the catalogue match these settings" instead of
spending a download.

Each museum's CDN still gets its own pacing (`REQUEST_GAP_MS = 750` between
requests to the same host) and the same cookie handling as before, and a 403
or 429 is still a refusal aimed at this client rather than a complaint about
one picture, so it's rethrown as `Refused` instead of trying the next
candidate — hammering a client a CDN is already throttling would only extend
the block. The message names the museum being asked ("The Metropolitan Museum
of Art is refusing requests from this phone for now — try again later")
rather than speaking of "a museum" in general, since it's the sentence a
person actually reads if this reaches the screen.

The default candidate pool is **Europe and Asia**. Settings can instead select
any combination of Europe, Asia, Africa, North America, South America and
Oceania — the same origin categories `regions.py` sorts every source's rows
into, not artist nationalities — or leave Origins at **Any**, which admits every
region. See **The filter model** below for how Origins, Shape, Subjects and
Artists combine. How much a region actually has to offer depends on which
museum contributed it: SMK's collection is almost entirely European, the
Cleveland Museum of Art's open-access paintings skew heavily Asian, and the
National Gallery of Art splits between Europe and North America — so pooling
four museums instead of one changes what's available for a given
region/subject/shape combination far more than it changes any single one's
share (see **Availability** below).

## The filter model

Settings has four sections — **Shape**, **Origins**, **Subjects** and
**Artists** — and a painting is offered only when it passes every one of them:
sections are **ANDed together**, while the options checked within one section
are **ORed** (checking both Landscape and Seascape widens Subjects to either).
**An empty selection in a section means Any: that section filters nothing.**
Shape already had this in `ArtworkShape.ANY`; Origins, Subjects and Artists each
get their own explicit **Any** option, always the first and only always-shown
row in their list. `Catalogue.matching` is the one place that applies all four —
`candidates` shuffles what it returns, and `Museums.fetch` downloads from that
shuffled list directly, with no per-choice draw or fallback pass: picking Vrubel
with Subjects at Any gives only Vrubel; picking Vrubel with Landscape gives only
Vrubel's landscapes, because both sections must pass at once. The portrait
exclusion and the religious-scene toggle (see below) apply to every entry
regardless of which sections it passed.

Each of the four sections in Settings is **foldable**: a header names the
section and summarises its current selection ("Europe, Asia", "Any subject",
"Mikhail Vrubel"), and tapping it folds or unfolds the option list beneath.
Fold state survives rotation (`rememberSaveable`); all four start unfolded
except Artists, which starts folded when it's at Any, since there's nothing to
review in a section nobody has narrowed.

Settings can choose any combination of Landscape, Seascape and Still life —
`ArtworkSubject` in `WallpaperPreferences.kt`, defaulting to Landscape alone on a
fresh install, Any (empty) being equally valid afterwards. Each subject stands
for several match queries rather than one: Landscape also matches "cityscape",
"city" and "street" — town and street views were once their own subject and now
fold into Landscape's own pool — Seascape also matches "marine" and "boats",
Still life also matches "flowers".

SMK's records come back in Danish (see **External services** in `CLAUDE.md`), so
every query list also carries the Danish words for the same subject — "landskab"
and "udsigt" for Landscape, "havn" and "strand" for Seascape, "blomster" for Still
life, and so on; a plain Danish title never contains the English word at all.
Danish "by" (town) is deliberately left out of Landscape's list: it collides with
the English preposition "by", which would flood the Met, NGA and Cleveland's
English-language titles with false matches. See the doc comment on
`ArtworkSubject` for the full lists and reasoning.

## Artist choice

Settings can also choose specific artists — one chip per name `Catalogue.artists()`
finds in the catalogue, currently just Mikhail Vrubel. `WallpaperPreferences.artworkArtists`
holds the choice as plain strings rather than an enum like `ArtworkSubject`, because
the artist list is catalogue data, not a fixed set this app defines; an artist name
that no longer appears in a later catalogue is simply tolerated and matches nothing,
never dropped from the stored preference the way a retired subject or region name is
(see `artistValues` in `WallpaperPreferences.kt`). Artists defaults to Any even on a
fresh install — there is no curated artist default the way there is for regions and
subjects.

Choosing an artist narrows Artists to that artist's own work, same as any other
section — combined with Subjects at Landscape, only that artist's landscapes
qualify, because both sections must pass (see **The filter model** above). This
is a deliberate change from an earlier build, where a chosen artist and a chosen
subject were alternatives (either counted): three turns in four came out as
whatever the largest OR-ed pool was, drowning out a specific artist choice.

The artists themselves come from `catalogue/artists.json`, a small config
`catalogue/build.py` reads to pull specific Wikimedia Commons categories into the
build (one row per artist, its region and the Commons categories that hold their
work) — a fifth source (`wmc`) alongside the four museums, distinguished in
`paintings.tsv` by a twelfth `artist` column the other four leave empty. See
`catalogue/build.py` for the pipeline side of this; `Catalogue.kt` only ever reads
the column.

### Availability

Some subject/region/shape combinations have nothing behind them at all. A
landscape painting runs wide far more often than tall, so Landscape and
Phone-shaped rarely coincide — checked against a build made 2026-09-25 (SMK,
NGA and Cleveland; the Met pass was still running): not one of the roughly 620
paintings Landscape matches across Europe — English or Danish — is
phone-shaped. Europe + Landscape + Phone-shaped is expected to stay empty, or
close to it, even once the Met's own paintings join the list — the format
problem is about what a landscape *is*, not which museum photographed it.

`Catalogue.availableRegions`, `availableSubjects` and `availableArtists` each
answer, for one section, which of its options still have at least one candidate
— checking an option **alone within its own section, with the other three
sections held at whatever Settings currently has staged**. Choosing Landscape
alone in Subjects while Vrubel is staged in Artists asks whether Vrubel has any
landscapes, not whether the catalogue has landscapes at all. Each is checked one
option at a time with `Sequence.any`, stopping at the first match, rather than
building `candidates`' full shuffled list — only presence, not the entry,
matters here. Settings recomputes all three, and `anyMatch` besides, off the
main thread on every staged change to any section, and hides an option a
section's availability call doesn't return; **Any is always shown**, whatever
the other sections are staged to.

When nothing at all passes every currently staged section, `Catalogue.anyMatch`
says so, Settings shows "Nothing matches these filters — set one section to
Any", and Apply stays disabled until something changes — the always-visible Any
option in every section is the guaranteed way out of that state. `Museums.fetch`
asks the same question by way of `candidates` coming back empty, and throws "No
paintings match these filters" — there is no separate availability check to run
first, because an empty `candidates` already means the same thing.

The religious-scene filter is a toggle, off by default, and sits below the four
sections rather than inside one of them, since it applies to all of them at
once. When it's on,
`Catalogue.candidates` skips a candidate whose title or tags match
`isReligious`'s word list: Christ, Mary and the saints, biblical scenes and
figures, a few non-Christian equivalents (Buddha, bodhisattva, deities), and a
short run of Danish terms for the same handful of subjects ("kristus", "jomfru
maria", "helgen", "apostel", "engel", "korsfæstelse" — "madonna" is already the
same word in both languages) — matched as whole words, case-insensitively, so
"Christmas" is never mistaken for "Christ". "St." is left out of the list on
purpose: it would also hide views of St. Petersburg and similar townscapes, and
the museums' own tags ("Saints", "Virgin Mary", "Christ", "Angels") catch most
of what excluding it gives up. The word list, and the matching portrait
exclusion (`isPortrait`, itself widened with Danish "portræt"), live in
`Catalogue.kt` — the one place that makes this judgment, now that there is no
live check left to keep in step with it. The desktop keeps its own copy of the
same lists in `src/art/museums.rs`, and iOS in `Catalogue.swift`.

## Placement is baked into the pixels

`WallpaperRenderer` always produces a bitmap exactly the size of the screen,
rather than leaving placement to Android. Zoom centre-crops, Stretch scales each
axis, Blur either puts a sharp fitted painting over a blurred fill or blurs the
whole fill, and Borders fits the painting over black, a custom colour or a colour
averaged from its edge. The Settings preview uses this same renderer at a smaller
size, so it cannot disagree with the applied result. The 1.25× enlargement limit
still prevents a small source from becoming a soft wallpaper.

Blur runs on a 256-pixel-wide intermediate bitmap with three box-blur passes.
That keeps the saved 0–100 strength control usable on API 30 without adding a
library or relying on API 31's `RenderEffect`. Custom borders use a colour wheel,
curated swatches and five common colours quantized from the current painting.

The screen size comes from `DisplayManager`'s current display mode, not
`WindowManager`, because the daily rotation runs from a `JobService` with no
Activity to ask. Home and lock screens receive the same bitmap in one call.

## Activity and Settings UI

The activity always opens on the existing dark artwork view. **Next picture**
stays above the bottom navigation instead of falling below the tall preview and
requiring a scroll. A two-segment artwork/settings control sits above the gesture
area; its final geometry is a compact rounded rectangle about 115 dp wide and 50
dp high, with 8 dp selected-segment corners rather than a capsule silhouette.

While a fetch is running, a 220 dp progress bar — the same width as the button —
appears between **Next picture** and the status line. It's indeterminate while
`Museums.fetch` is choosing among the local catalogue, because that step is a
local filter with no length to report; it switches to determinate, tracking
bytes downloaded against the response's `Content-Length`, once a candidate is
actually downloading. A pixel mismatch discards that download and starts the
next candidate's from zero, up to `MAX_ATTEMPTS`, rather than the bar going
back to indeterminate — there is no separate checking step left to return to.
The status line narrates the same progress in words ("Choosing a painting…",
"Downloading 4.2 of 12.0 MB…") while the button itself just says "Fetching…".
This also appears when the
scheduled job runs the fetch, since `Rotation` is a singleton in the same process
`MainActivity` reads its `StateFlow` from.

Settings uses the same near-black gallery atmosphere as the artwork view. Its
wallpaper preview is a centred 150 dp-wide phone frame rendered at the physical
screen's aspect ratio. The four placement modes live in one slim horizontal
strip, with extra blur or border controls revealed only for the selected mode.
Blur preview rendering has no release-time debounce: every changed slider value
updates the preview while the thumb is moving. Below that, Shape, Origins,
Subjects and Artists are foldable sections — see **The filter model** above.
Settings remain staged until **Apply changes** re-renders the current cached
painting and saves them.

The heart in the artwork preview copies the shown painting into durable app
storage. **View favourites** opens a full-screen lazy gallery whose visible
cards hold sampled thumbnails; selecting one provides a larger preview plus
*Set as wallpaper* and *Remove*. A chosen favourite changes the shown wallpaper
without replacing the source painting recorded for the day, so normal rotation
resumes on the next local day. If a favourite is chosen while a painting is
already owed, that choice settles the day so an overdue job cannot immediately
undo it.

## Scheduling

- **`JobScheduler`, not WorkManager.** It's the OS's own scheduler with no
  library to add, matching the project's habit of reaching for what the
  platform already provides before adding a dependency.
- **The daily job runs hourly, unmetered, and persisted** across reboots. Each
  run does nothing but ask `State.isDue` — a comparison of `LocalDate` epoch
  days, exactly the desktop's `State::is_due` rule: compare calendar days,
  never count down and never count hours. The painting changes on the first
  hourly check after midnight on Wi-Fi. Unmetered is the default because an
  original download runs 2–30 MB.
- **`Next picture` is a one-off job on any network**, and it spends the day —
  the same rule as the desktop's `record_fetched`: a picture that came from the
  source is the day's picture, whatever hour it was asked for.
- **`ACCESS_NETWORK_STATE` is required.** Android 16 refuses to schedule a job
  with a network constraint without it, and throws while doing so. The
  permission is granted at install without a prompt, but the build and unit
  tests cannot see it missing; the first real launch crashed in
  `MainActivity.onCreate`.
- **One wallpaper operation at a time.** Rotation and Settings Apply share the
  same try-lock, so a fetch and a re-render cannot race for the wallpaper or cache.
- **No libraries beyond Compose, core-ktx and coroutines.** HTTP, JSON,
  scheduling and decoding all come from the OS. Besides the habit, the
  development machine sits behind a TLS-intercepting proxy that can stop Gradle
  fetching artifacts it has not already cached.

## Verification

Android builds run on `ios_macmini`, following the same cache-preserving rsync
workflow as Calendar Puzzle:

```sh
./remote.sh             # tests and assembles on the mini
./remote.sh check       # compileDebugKotlin only
./remote.sh apk         # build and fetch android/out/ArtWindow.apk
./remote.sh <tasks...>  # run custom Gradle tasks remotely
```

The remote wrapper uses the mini's unpacked JDK 17 and Android SDK, preserves
Gradle build state between syncs, and always stops the Gradle daemon. The APK is
fetched back because the phone remains attached to this machine.

With a phone attached over USB debugging:

```sh
./remote.sh apk
./android/install.sh
adb shell dumpsys wallpaper   # confirms the stored wallpaper's size
adb logcat -s ArtWindow       # the fetch and placement trail
```

## Television

The same APK installs on Android TV and Google TV (`leanback` and `touchscreen` are
declared optional). A TV is not a separate module, because nothing that chooses,
downloads or schedules a painting knows what device it is on; only two things
differ. First, the screen is landscape: `Screen` keeps the orientation the display
reports rather than sorting it portrait, so Screen-shaped means *wide* there, and
Include near-square spans 0.8 up to the screen's own ratio plus `MAX_TRIM`. The
phone's empty Europe + Landscape + Phone-shaped pool is the TV's richest one.

Second, there is no wallpaper to set. `Wallpaper.pin` branches once on
`isTelevision()` and publishes the rendered bitmap to `Frame` instead of
`WallpaperManager`; `Frame.current` re-renders from `State.shownArtwork` after the
process has been killed. Two surfaces observe it:

- **`ArtDream`**, a `DreamService` screensaver: the Frame-like "art when the TV
  is idle". It crossfades to a new painting and shows the caption for eight
  seconds. If a painting is owed it asks `RotationJob` for one and never
  fetches itself.
- **`TvActivity`**, the art mode on the home screen's app row: the painting full
  screen with the screen kept on. OK opens an overlay with Next picture,
  Favourite, Favourites, Settings and *Use as screensaver*. Settings and
  Favourites are the phone composables with D-pad focus rings (`focusRing`). The
  colour wheel is hidden, so borders are chosen from the swatches.

Google TV hides third-party screensavers from its settings, so *Use as screensaver*
falls back to showing the command that selects it:

```sh
adb shell settings put secure screensaver_components dev.artwindow/.ArtDream
adb shell am start -n com.android.systemui/.Somnambulator   # start it now
```

## Out of scope for now

The folder source remains desktop-only. The crop and enlargement safety limits
remain named constants rather than user-facing tuning.
