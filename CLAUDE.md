# Art Window — working notes

A daily public-domain painting as the desktop wallpaper, fit to screen and
letterboxed in black, run from a menu bar icon. macOS only so far; `wallpaper/` is
shaped for Windows and Linux backends that do not exist yet.

## Commands

```sh
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
./macos/bundle.sh          # -> target/Art Window.app
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
  `MainThreadMarker`. It errors rather than trusting a doc comment. This is why
  `rotation` is split: `fetch` blocks for up to two minutes and runs on a worker,
  `show` touches AppKit and runs on the event loop's thread.
- **The scheduler compares wall-clock times; it never counts down.** `State::is_due`
  against `now_secs()` is the only thing that decides a picture is owed. A timer
  cannot survive a closed lid, and a machine asleep at the appointed moment is the
  normal case. The tray's `TICK` only decides how *often* to ask the question.
- **Every failure path must set `hold_until`.** The day is marked done only on
  success, so an error with no cooling-off period retries instantly and forever.
- **`state.last_success` advances only after a *successful* fetch.** A failed
  network call must not consume the day; the next run retries. `State::record_shown`
  is the only way to move it: stamping the clock, remembering the picture and
  writing the file are one operation, so there is no longer a way to do half of it
  from outside.
- **The cache filename `met-{id}.{ext}` is load-bearing.** `met::id_of` reads the
  object id back out of it twice over: to keep tomorrow's painting from being
  today's, and to decide which files in the cache are the Met's to delete. Renaming
  downloads breaks both silently — nothing errors, the same picture just comes round
  again. The id is derived rather than stored so it cannot drift out of agreement
  with the file actually on screen, and `id_of` is private so the convention cannot
  escape `art/met.rs`.
- **`config.toml` is read, never written. `state.json` is written, never read by a
  human.** Two files because they have two authors — serialising config back would
  destroy the user's comments. The `source` string is decoded into `SourceSpec`
  while the file is read, so nothing downstream ever handles it as text.
- **A source owns everything about itself; `rotation` branches on nothing.**
  `Source::fetch` is handed the whole previous `Artwork` rather than an identifier,
  because only a source knows how it recognises its own work — the Met parses an
  object id out of a filename, a folder compares paths. `discard_all_but` is that
  same rule for deletion: whoever wrote a file is the only one who may decide it is
  rubbish, which is why a folder of the user's own pictures cannot be pruned by
  mistake. `SourceSpec` owns how the choice is spelled, and `source_for` owns which
  implementation answers to it. Adding a third source should touch `art/` and
  nothing else — an `if config.source == …` outside `art/mod.rs` means the seam has
  been broken again.
- **Nothing decodes image pixels.** `Artwork` carries a `PathBuf`; the file goes
  straight to the OS. This is why there is no `image` dependency. Do not add one
  without a reason that survives the question "does the OS not already do this?".
  The menu bar glyph is the one bitmap in the program and it is drawn from ASCII
  art in `tray.rs` for exactly this reason — `tray_icon::Icon::from_path` is
  Windows-only, so the alternative was a decoder for an eighteen-point icon.

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

## The menu bar app

`tao` + `tray-icon` + `muda`, which pin the same `objc2 0.6` / `objc2-*-0.3` family
this project already used, so there is one copy of AppKit in the tree. `tray-icon`
needs a **GTK** event loop on Linux, which is why `tao` is the pairing and `winit`
is not.

Three things that are not guessable from the docs:

- The `TrayIcon` must be built inside `Event::NewEvents(StartCause::Init)`, not
  before `run()`, or it goes missing in front of full-screen apps. Afterwards the
  main `CFRunLoop` needs a manual `wake_up()` or the icon does not appear at all.
- `set_activation_policy(Accessory)` takes `&mut EventLoop` and only works *before*
  `run()`. It hides the Dock icon for an unbundled `cargo run`; the bundle's
  `LSUIElement` does the same thing earlier and without the bounce. Both are set.
- Dropping the `TrayIcon` is what removes it from the menu bar, so Quit does
  `tray.take()` before `ControlFlow::Exit`.

**Start at login is a launchd agent, not `SMAppService`.** `SMAppService.mainApp` is
the modern answer and needs macOS 13; the development machine runs 12.7. The agent is
written and deleted directly and `launchctl` is never called — bootstrapping it would
start a second copy of a program that is by definition already running, and the
setting only means anything at the next login anyway.

## Planned, not built

The Windows and Linux backends. `autostart.rs` is macOS-only and would grow the same
platform split `wallpaper/` has.
