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

The menu can be driven from a script, up to a point. Open it with

```sh
osascript -e 'tell application "System Events" to tell process "art-window" \
  to (click menu bar item 1 of menu bar 1)'
```

then walk `menu 1 of menu bar item 1 of menu bar 1` to read every row's name and
`enabled`. Clicking a **top-level** row works and really does reach the program.

**Clicking a row inside a submenu does not.** The tree reads correctly and
`perform action "AXPress"` returns success, but no `MenuEvent` ever arrives — checked
by logging every id the event loop received. The menu has no submenus left, so
nothing is currently caught by this; it is written down because the next person to
add one will lose an afternoon to it otherwise.

The favourites window can be driven the same way, up to the same sort of line.
`buttons of window 1` finds *Set as wallpaper* and *Forget*, and `perform action
"AXPress"` on them really does reach the program — that is the way to test showing
and forgetting from a script. **Clicking a thumbnail cannot be faked.** The shelf is
one custom-drawn view with no accessibility children, so there is nothing to press,
and a synthetic `click at {x, y}` does not reach it either: `hitTest:` is asked (the
cursor moving is enough for that) but no `mouseDown:` ever follows. Real clicks from
a real mouse work perfectly well. To test what happens *after* a picture is chosen,
press the button rather than the thumbnail.

Three smaller traps. The menu stays open when the script ends, so the *next* click
closes it rather than opening it — send `key code 53` before returning, and if a read
fails with "invalid index", that is why. The position AppleScript reports for the
status item is not to be believed: it read as far off-screen while clicks on it were
landing perfectly well. And the window does not open in the same place twice, so read
`position of window 1` in the same run you click in — coordinates from the last run
will land on the desktop and quietly do nothing.

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
  `rotation` is split: `fetch` blocks for a couple of minutes and runs on a worker,
  `show` touches AppKit and runs on the event loop's thread. `fetch` is bounded by
  `met::BUDGET` rather than by its per-request timeouts alone, because the tray parks
  its clock entirely while a fetch is in the air — a chain of seventeen requests with
  no overall limit is a menu bar reading "Fetching…" for half an hour and no tick
  scheduled behind it.
- **The scheduler compares calendar days; it never counts down and never counts
  hours.** `State::is_due` asks `day::local` whether the date has changed since
  `last_success`, and that is the only thing that decides a picture is owed. Both
  halves of that are load-bearing and both were once wrong. An *interval* — the
  original `refresh_hours` — drifts, because a machine asleep past the appointed
  moment settles the day whenever it wakes and that becomes the new anchor; left
  alone the changeover walks right around the clock. A *countdown* cannot survive a
  closed lid at all, which is why the tray's `TICK` decides only how often to ask the
  question, and `wake::watch` exists to ask it the moment the lid opens: `Instant`
  does not advance while macOS sleeps, so nothing monotonic can be trusted to notice
  midnight. Every deadline in `tray.rs` is therefore wall-clock seconds — `hold_until`
  included.
- **Every failure path must set `hold_until`.** The day is marked done only on
  success, so an error with no cooling-off period retries instantly and forever.
- **`state.last_success` advances only when the day is actually settled.** A failed
  network call must not consume the day; the next run retries. Two methods may move
  it and no others: `State::record_fetched` always, because a picture arrived, and
  `State::record_chosen` only when `is_due` already said one was owed. Each stamps
  the clock, remembers the picture and writes the file as one operation, so there is
  no way to do half of it from outside.
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
- **A favourite is a copy, and the copy is the whole point.** The cache holds one
  picture: `discard_all_but` deletes every other download the moment a new one
  goes up. Remembering a path would remember a file that is already gone, which is
  why `Favourites::keep` copies into a folder of its own before recording
  anything.
- **`Favourites` owns its folder exactly as a source owns the cache.** Its
  `discard_all_but` has the same name, the same contract and the same reason: only
  whoever wrote a file may decide it is rubbish, and the file on the desktop is
  never rubbish. That exception is what lets `forget` drop the very picture on
  screen without blanking it — the row leaves the menu at once, the file waits
  until the desktop is pointing somewhere else. The sweep therefore has to be run
  wherever the desktop changes — after a forget, after a fetch and after a hand-pick
  — which is why `tray` calls it in all three.
- **The copy keeps the original file name.** `met::id_of` reads the object id back
  out of it whatever folder the file sits in, so tomorrow's painting still avoids
  being the favourite already on the desktop. Two pictures out of someone's own
  folder can collide, and then one is renamed; a folder recognises its work by
  path, so nothing is lost by it.
- **A picture chosen by hand takes the desktop without taking the day.** Showing a
  favourite, or putting the day's own picture back, leaves `state.fetched` and the
  clock alone, so tomorrow's painting still arrives at its usual hour however often
  the desktop is changed in between. The one exception is a picture that was already
  owed: then the choice settles it, because otherwise the tail of the event loop
  would spawn the overdue fetch seconds later and take the desktop straight back.
  `is_due` is still the only thing that decides a picture is owed — `record_chosen`
  asks it rather than second-guessing it. A download already in the air is dropped
  rather than hung — see `superseded` in `tray.rs` — because landing it would undo
  a choice just made. *Next picture* is the other way round and deliberately so: a
  picture that came from the source is the day's whatever hour it was asked for, so
  it goes through `record_fetched` like any other rotation. Choosing among pictures
  that already exist leaves the day alone; going back to the museum spends it.
- **Only the tail of the event loop starts a fetch.** A click cannot spawn one where
  it is answered — the tail is what decides whether the loop then waits, holds or
  ticks, and a worker started behind its back leaves it deciding against a stale
  `fetching`. So *Next picture* raises `asked_for_next` and the tail reads it,
  jumping both the cooling-off period and the schedule. The row is greyed while a
  download is in the air rather than the request being queued: two workers racing
  for the desktop would leave the loser writing into a cache the sweep had already
  been run for.
- **The source's sweep spares `state.fetched`; the favourites' sweep spares
  `state.shown`.** Getting these the wrong way round is not a hypothetical: it is
  the bug that made coming back to today's picture impossible, because handing the
  favourite's path to `Source::discard_all_but` told the Met to spare a file that
  was never in its cache, and today's download was deleted. The rule behind both is
  the same — whoever wrote a file may delete it, but never the one on the desktop,
  and never the one there is still a way back to.
- **The window holds thumbnails, not paintings.** `gallery` decodes one full
  picture at a time to make each thumbnail and lets go of it before reading the
  next, so a shelf of any length costs one painting's worth of memory to build
  rather than the whole folder's. The only picture kept at full size is the one
  being looked at, and handing the view the next is what releases the last. A
  thumbnail is made once and cached under its `Favourites` key — by key and never by
  position, because a list rebuilt around a deletion would otherwise pair a painting
  with somebody else's picture.
- **A thumbnail is drawn at twice the size it is shown at.** Many pixels, few
  points, which is what a sharp image on a Retina display *is*. This is why
  `thumbnail` builds an `NSBitmapImageRep` of a stated pixel size and then tells it
  it measures the smaller amount, rather than using `NSImage::lockFocus` — locking
  focus draws at whatever the screen happens to be, so the same favourite would come
  out crisp or blurred depending on which display the window was opened on. It is
  deprecated besides, but that is the lesser reason.
- **A click in the window is answered by the loop, not where it lands.**
  `mouseDown:` and a button's action arrive mid-click on AppKit's thread, which owns
  nothing the loop does. They send a `Pick` through the same `EventLoopProxy` the
  menu and the wake notification use. `Pick` has two variants and not three because
  selecting a picture changes nothing outside the window and so has no business
  leaving it.
- **The shelf accepts the first mouse.** `acceptsFirstMouse:` returns true, and it
  has to: this program is an `Accessory` and its window is hardly ever the active
  one, so the ordinary rule — the first click into an inactive window only wakes it —
  would put an extra click in front of every visit.
- **The menu and the window are one list, told once.** `Ui` owns the `Gallery`, so
  every `ui.describe(…)` keeps both surfaces honest and no caller has to remember
  the second. The window is told even while it is shut, because the alternative is
  for callers to know whether it is open and the one that got it wrong would leave a
  painting on screen that had already been thrown away.
- **The loop classifies, then acts, then schedules.** `match event` says what was
  asked, `match wanted` does it, and the clock at the tail is unchanged. The three
  are separate because the same two things — show this, forget this — can be asked
  from a menu row or from the window, and answering them in two places is how the
  two would drift apart. This is also why `Wanted` is reached through
  `From<Pick>` rather than the window knowing what a `Wanted` is.
- **Nothing decodes image pixels.** `Artwork` carries a `PathBuf`; the file goes
  straight to the OS. This is why there is no `image` dependency. Do not add one
  without a reason that survives the question "does the OS not already do this?".
  The menu bar glyph is the one bitmap in the program and it is drawn from ASCII
  art in `tray.rs` for exactly this reason — `tray_icon::Icon::from_path` is
  Windows-only, so the alternative was a decoder for an eighteen-point icon.
- **A day is a local day, and the OS is asked what that means.** `day::local` is the
  whole calendar this program has: one number per instant, comparison the only
  operation. The UTC offset comes from `NSTimeZone` for *the instant in question*
  rather than for now, so the hours either side of a daylight-saving change do not
  read as the wrong day. No date crate — the question "does the OS not already do
  this?" has the same answer here as it does for image decoding.
- **`config.toml` may name settings that no longer exist.** `Config` has
  `deny_unknown_fields` so a typo is an error rather than a silent shrug, which means
  a retired setting cannot simply be deleted: every file written by an earlier
  version would fail to parse, and it would fail before there is a menu bar to say so
  in. `refresh_hours` is therefore still a field, typed `IgnoredAny`. Retiring a
  setting means moving it to that, not removing the line.

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
  `tray.take()` before `ControlFlow::Exit`. The favourites window is the same rule
  and Quit says it too — see `Gallery::dismiss`.
- `muda` fires a menu event from the item's own id and says nothing about where it
  sat, so a row nested two submenus deep arrives exactly like a top-level one. The
  favourites submenu is gone — the window replaced it — but that is why it worked
  while it was there, and why anything nested added later will not need handles.

**The machine waking is a notification, not something to poll for.**
`NSWorkspaceDidWakeNotification` arrives on `NSWorkspace`'s own notification centre —
the default `NSNotificationCenter` never sees it — and `wake::watch` forwards it to
the loop through the same `EventLoopProxy` the menu uses, for the same reason: it
arrives on somebody else's terms and is no place to touch the state the loop owns.
Its match arm is deliberately empty. The clock at the tail of the loop is re-read
after *every* event, so arriving is the entire message; putting work in the arm would
be duplicating what the tail already does.

**The favourites window is tao's, and only what is inside it is AppKit's.** A
`WindowBuilder` buys the title bar, the close button arriving as
`WindowEvent::CloseRequested`, resizing, and `Window::set_focus()` — which already
does `makeKeyAndOrderFront:` followed by `activateIgnoringOtherApps:`, the pair an
`Accessory` app needs to put a window in front of anything. `gallery/macos.rs`
reaches through `WindowExtMacOS::ns_view()` and fills it, and lays its own container
inside that so the arithmetic is written in coordinates this program decides the
orientation of rather than tao's. Everything resizes by autoresizing mask, so
`WindowEvent::Resized` never has to be handled at all.

Closing the window is not quitting, and its arm deliberately does not `return` — the
clock at the tail of the loop still has to be wound.

**Start at login is a launchd agent, not `SMAppService`.** `SMAppService.mainApp` is
the modern answer and needs macOS 13; the development machine runs 12.7. The agent is
written and deleted directly and `launchctl` is never called — bootstrapping it would
start a second copy of a program that is by definition already running, and the
setting only means anything at the next login anyway.

## Planned, not built

The Windows and Linux backends. `autostart.rs` is macOS-only and would grow the same
platform split `wallpaper/` has. `gallery/` already has that split — the window
itself is tao's and portable, and only what is drawn inside it is AppKit — but like
`wallpaper/` it names no backend for anywhere else, so the crate still does not build
off macOS.
