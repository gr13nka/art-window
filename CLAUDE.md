# Art Window — working notes

A daily public-domain painting as the desktop wallpaper, fit to screen and
letterboxed in black. macOS only so far; `wallpaper/` and `autostart/` are shaped
for Windows and Linux backends that do not exist yet.

## Commands

```sh
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

There are **no tests**. `cargo test` exits 0 because the suite is empty — it is not
evidence of anything. Verification is done by running the binary and reading the
Dock's store (see `docs/macos-wallpaper.md`).

## The one thing that will waste your afternoon

`NSWorkspace.setDesktopImageURL` **returns success while changing nothing you can
see.** macOS keeps a wallpaper per Mission Control Space per display, and that call
reaches only the Space active for the calling process — 2 slots out of 49 on the
development machine. `wallpaper::pin` therefore also writes the Dock's private
SQLite store and restarts the Dock.

Full details, schema and the four traps in it: **`docs/macos-wallpaper.md`**. Read
it before touching `src/wallpaper/macos.rs`.

## Invariants

- **`wallpaper::pin` owns re-asserting placement.** macOS records placement per
  image *path*, so every new file arrives with the system default (crop-to-fill).
  Callers must never be responsible for re-applying — forgetting that is the
  original bug this project exists to fix.
- **`pin` must run on the main thread.** `NSScreen::screens` demands a
  `MainThreadMarker`. It errors rather than trusting a doc comment.
- **`state.last_success` advances only after a *successful* fetch.** A failed
  network call must not consume the day; the next run retries.
- **`config.toml` is read, never written. `state.json` is written, never read by a
  human.** Two files because they have two authors — serialising config back would
  destroy the user's comments.
- **Nothing decodes image pixels.** `Artwork` carries a `PathBuf`; the file goes
  straight to the OS. This is why there is no `image` dependency. Do not add one
  without a reason that survives the question "does the OS not already do this?".

## Deliberate omissions

Reversing these needs a reason, not a tidy-up impulse.

- **No blur/dim effects.** The app this replaces had them; its user never enabled
  them once. They cost a module, the `image` crate and two menu items.
- **No landscape filtering of artwork.** Fit-plus-black-letterbox renders a portrait
  painting as a framed picture on a black wall, which is the intended look. The
  display policy removed the need for the filter.
- **Not the Art Institute of Chicago.** Its metadata API is fine, but the image host
  `www.artic.edu/iiif/...` sits behind a Cloudflare managed challenge that an
  unattended client cannot answer. The Met has no such gate. Do not switch back.

## External services

The Met's API needs a real `User-Agent`; anonymous traffic is what earns bot
challenges. One request per day. `art/met.rs` filters to department 11 (European
Paintings) — an unfiltered collection of 490,000 objects is mostly coins and
textiles.

## Planned, not built

Tray icon (`tao` + `tray-icon` + `muda`), an in-process scheduler, and the Windows
and Linux backends. Note for whoever does the tray: `tray-icon` needs a **GTK** event
loop on Linux, so `tao` is the right pairing and `winit` is not — it speaks
X11/Wayland directly and provides no GTK loop.
