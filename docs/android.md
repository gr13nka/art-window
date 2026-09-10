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

What *could* be shared is smaller than it looks — the Met protocol in
`src/art/met.rs` is about 150 lines: build a search URL, walk candidates, decide
what to skip, download, name the file. Sharing that would mean putting it behind
`cargo-ndk`, building for four ABIs, and writing a JNI bridge to carry strings and
byte arrays across it — machinery bigger than the code it would share, for a
platform that already has `HttpURLConnection` and `org.json` built in.

The cost of not sharing it is that the Met's protocol now lives twice: once in
`src/art/met.rs`, once in `android/app/src/main/java/dev/artwindow/Met.kt`. A
change to the search query, the `User-Agent`, or the `met-{id}.{ext}` filename
convention belongs in both files, and each carries a comment pointing at the
other so that isn't easy to forget.

## Why the phone filters by shape when the desktop deliberately doesn't

The desktop's **No filtering by the shape of a picture** rule (see the main
`CLAUDE.md`) holds because fit-plus-letterbox already renders any painting well —
a tall canvas reads as a framed picture on a black wall regardless of its
proportions. A phone screen is roughly 0.45 units wide per unit of height, close
to nothing in the Met's collection. Letterboxing a landscape painting there
would leave a thin, useless strip down the middle. So the Android app fills the
screen instead of framing the picture, and to fill a screen that narrow without
grotesque cropping, it has to be selective about which paintings it will even
consider. `Screen.kt` is where that selectivity lives — see below.

## The numbers behind the pool

Measured 2026-09-10 by sampling the Met's API:

- Only about 1–4% of European Paintings (department 11) are phone-shaped
  (width/height between 0.40 and 0.53). Asian hanging scrolls run around 18% —
  a scroll is already a tall, narrow format.
- A `q=landscape` search against Asian Art (department 6) also returns ceramics,
  prints and textiles — nearly half of a 50-object sample. The search therefore
  asks for `medium=Paintings`, which removes them before a single lookup is
  spent. The `classification == "Paintings"` check on each record stays as a guard.
- Across the whole pipeline, about **one candidate in sixty** ends up fitting.
  The first run on a real phone looked at 120 and found none.

Because department 11 alone is too thin a pool at phone proportions, the
candidate pool is **both** department 11 and department 6, each searched with
`q=landscape` for the same reason the desktop does: a generic query returns
mostly portraits.

## Pre-filter, then verdict

The catalogue and the photograph do not always agree on proportions, and how
much they disagree depends on the department:

- For European paintings, the catalogue `measurements` "Overall" figure (in cm)
  matches the photograph's proportions to within about 0.03 — close enough to
  trust.
- For scrolls it does not. The catalogue lists several elements — "Image",
  "Overall with mounting", "Overall with knobs" — and the photograph is sometimes
  of the painted image and sometimes of the whole mounting, with nothing in the
  record saying which. So any one element can be well off from what the photo
  actually shows, and the pre-filter lets a candidate through if *any* element
  passes.

So each candidate passes three checks, cheapest first:

1. **Catalogue, with slack.** `Screen.mightHold` asks whether any catalogued
   element is close enough to be worth a look. It checks plausibility, not
   correctness.
2. **The web-sized copy's shape.** `Screen.holds` is run against
   `primaryImageSmall`, a few hundred kilobytes with the photograph's real
   proportions. In sampling, four of every five candidates that passed the
   catalogue failed here. They were folding screens and triptychs that list one
   tall panel among their measurements and are photographed whole. Without this
   step each one cost a full original download, up to tens of megabytes on a
   phone's data plan.
3. **The original's pixels.** `Screen.place` is run against the downloaded
   original's header, without decoding it. It adds the enlargement limit, which
   a web-sized copy cannot answer.

A candidate that fails any step is discarded, and the search moves to the next
one.

## Sequential lookups

Candidate object lookups run one at a time, not concurrently. This isn't a
theoretical caution: bursts of parallel requests against the Met's API got
blocked outright while sampling the numbers above. `Met.fetch` walks candidates
in order, up to `CANDIDATES = 400` or a `BUDGET_MS` of 3 minutes, whichever comes
first — the same shape as the desktop's own candidate loop and `BUDGET` in
`src/art/met.rs`, for the same reason: a chain of requests needs an end, or a
slow network turns into an indefinite wait with nothing telling the user why. At
one fit in sixty, 400 makes running dry rare, and the budget is what usually
binds.

When nothing fits, the error says so ("none of N paintings fit a W×H screen"),
and the last network error travels as its cause. The first version reported
that last error alone. On a phone, that turned 120 wrong-shaped paintings into
a misleading HTTP 404.

## Placement is baked into the pixels

`Wallpaper.pin` decodes the downloaded file straight into a bitmap sized to
exactly the screen — `ImageDecoder`'s target size plus a crop, computed once by
`Screen.place` — rather than handing the system a full-resolution image to crop
or scale itself. That leaves Android nothing to zoom, crop, or parallax-scroll
at display time: the bitmap already *is* the wallpaper, pixel for pixel. The
placement math enforces two limits so a mismatched painting is rejected rather
than mangled: at most 15% trimmed from either axis, and at most 1.25× enlargement.

The screen size comes from `DisplayManager`'s current display mode, not
`WindowManager`, because the daily rotation runs from a `JobService` with no
Activity to ask. Home and lock screens receive the same bitmap in one call.

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
- **One turn at a time.** `Rotation.turn` takes a `tryLock` rather than
  queuing, so a second trigger while a fetch is already running is simply
  dropped instead of piling up behind it.
- **No libraries beyond Compose, core-ktx and coroutines.** HTTP, JSON,
  scheduling and decoding all come from the OS. Besides the habit, the
  development machine sits behind a TLS-intercepting proxy that can stop Gradle
  fetching artifacts it has not already cached.

## Verification

With a phone attached over USB debugging:

```sh
./android/install.sh
adb shell dumpsys wallpaper   # confirms the stored wallpaper's size
adb logcat -s ArtWindow       # the fetch and placement trail
```

## Out of scope for now

Favourites and the gallery window, the folder source, and a settings screen.
`MAX_TRIM` and `MAX_ENLARGEMENT` are named constants in `Screen.kt` rather than
configurable values.
