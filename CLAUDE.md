# Art Window — working notes

A daily public-domain painting as the desktop wallpaper, fit to screen and
letterboxed in black. It runs as a macOS menu-bar app, a GNOME GTK application or
a Windows notification-area app;
platform wallpaper, browser and login behavior meet behind `desktop/`. A native
Kotlin Android app under `android/` sets the same daily painting as the phone
wallpaper instead — see `docs/android.md` — and a native Swift app under `ios/`
hands it to Shortcuts for iPhone and iPad — see `docs/ios.md`.

README.md is the landing page; depth lives in `docs/GUIDE.md`. Documentation edits
belong in the guide, not in the README.

## Commands

```sh
cargo build --release
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
./macos/bundle.sh          # -> target/Art Window.app
./macos/dmg.sh             # -> target/dist/Art-Window-<version>-macos.dmg (universal)
./linux/check-container.sh # Linux release build, tests, clippy and formatting
./linux/install.sh         # user-local GNOME binary, launcher and icon
./linux/package.sh         # -> target/dist/art-window-<version>-linux-x86_64.tar.gz
./windows/package.ps1      # on Windows: Inno Setup installer in target/dist/
cd android && ./gradlew testDebugUnitTest assembleDebug
./android/install.sh       # build debug APK and adb install -r
./ios/remote.sh            # iOS: xcodegen + unit tests on ios_macmini's simulator
./ios/remote.sh install    # iOS: signed device build, installed via devicectl
python3 catalogue/build.py  # regenerate the museum list (Met pass takes ~3 h, resumable)
```

Run `./gradlew --stop` after a Gradle build, and never run two Gradle builds at
once — the wrapper's memory limits assume only one is running.

Tests cover the platform-independent action translations and the Linux timezone and
autostart contracts. They do not make wallpaper integration a pure unit-testable
operation. Verify the real desktop too: read the Dock's store on macOS (see
`docs/macos-wallpaper.md`), run `art-window --check` inside GNOME (see
`docs/gnome-wallpaper.md`), or look at *Settings → Personalization → Background*
on Windows (see `docs/windows-wallpaper.md`).

Windows cannot be run from the development Mac, but it can be type-checked there:
`rustup target add x86_64-pc-windows-msvc`, then `cargo check --target
x86_64-pc-windows-msvc` using rustup's cargo rather than Homebrew's — which means
`~/.cargo/bin` first on `PATH`, not merely calling that `cargo` by its path: it
finds `rustc` on `PATH`, and Homebrew's has no Windows `core` to offer. `ring` needs
LLVM's `clang-cl` (`CC_x86_64_pc_windows_msvc=clang-cl`), plus stub `assert.h`,
`string.h` and `stdlib.h` headers on `CFLAGS_x86_64_pc_windows_msvc`, because no
MSVC CRT headers are installed. `build.rs` embeds the icon and manifest only on a
Windows host. `cargo clippy` takes the same target and environment, and a
`CARGO_TARGET_DIR` of its own keeps it from fighting the host build for the lock.

Neither Mac runs Docker, so `./linux/check-container.sh` cannot be run from
either and the GNOME code is first compiled by CI's Linux job. Android and iOS
build on `ios_macmini` through `./remote.sh` and `./ios/remote.sh`.

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
development machine. `desktop::pin` therefore also writes the Dock's private
SQLite store and restarts the Dock.

Full details, schema and the four traps in it: **`docs/macos-wallpaper.md`**. Read
it before touching `src/desktop/macos/wallpaper.rs`.

## Invariants

- **`desktop::pin` owns re-asserting placement.** macOS records placement per
  image *path*, while GNOME stores it in `org.gnome.desktop.background`. Every
  backend sets fit, black margins and the URI together. Callers must never be
  responsible for re-applying—forgetting that is the original bug this project
  exists to fix.
- **A picture reaches the Space in front of the user at once, and the rest on the
  loop's terms.** `pin` answers with all three of `Pinned`, and `tray::Owed` holds
  what each one leaves owing. `InPart` is the Dock's store refusing the write — at
  login the ordinary case, because the Dock is still building it and neither side
  waits for the other's lock. The rotation still spends the day, since the painting
  *did* arrive and re-downloading it would not help; the same picture is offered
  again a minute later, five times over, and beginning a session owes one asking
  whatever the state file says. Nothing else would ever put it right: the day is
  settled, so no schedule returns to it until tomorrow.
- **Publishing to the other Spaces is deferred, because it blanks the desktop.**
  Writing the Dock's store changes nothing anyone can see; only restarting the Dock
  does, and every desktop is blank for as long as that takes — half a minute on the
  development machine. So `pin` writes and answers `Pinned::AfterRedraw`, and
  `desktop::catch_up` is the restart, called only where the desktop was going to be
  redrawn anyway: waking, and beginning a session. Never at the moment a picture
  changes — the Space being looked at is already right by then, and the ones waiting
  are by definition not being looked at. The two exceptions are *Re-apply the
  wallpaper*, which is a request for exactly that disruption, and `--once`, which has
  no next redraw to wait for and a terminal in front of it.
  The deferred debt names its picture: a later `InPart` clears an older picture's
  pending redraw, so catching up can never publish a superseded wallpaper.
- **The Dock gives up its store when it loses the lock to this program.** It
  renames the file to `desktoppicture.db.corrupt` and starts an empty one, and the
  next restart publishes that to every Space. So `pin` commits the store *before*
  it calls `NSWorkspace`, whose answer is the Dock writing the same file; the tail
  of the loop neither presses nor restarts when a download is in the air or about
  to start; and waking asks for a waiting write again (`Owed::recheck`) before the
  restart that would show it. See
  `docs/macos-wallpaper.md#the-dock-throws-the-store-away-when-it-loses-the-lock`.
- **The macOS `pin` backend must run on the main thread.** `NSScreen::screens`
  demands a `MainThreadMarker`. It errors rather than trusting a doc comment. This is why
  `rotation` is split: `fetch` blocks for a couple of minutes and runs on a worker,
  `show` touches AppKit and runs on the event loop's thread. `fetch` is bounded by
  `met::BUDGET` rather than by its per-request timeouts alone, because the tray parks
  its clock entirely while a fetch is in the air — a chain of seventeen requests with
  no overall limit is a menu bar reading "Fetching…" for half an hour and no tick
  scheduled behind it.
- **The scheduler compares calendar days; it never counts down and never counts
  hours.** `State::is_due` asks `day::local` whether the date has changed since
  `last_success`, and that is the only thing that decides a picture is owed. A day
  begins at 05:00 local (`day::DAY_BEGINS`), not midnight: a picture fetched at
  half past midnight used to be the "old" one waiting the next morning. Both
  halves of that are load-bearing and both were once wrong. An *interval* — the
  original `refresh_hours` — drifts, because a machine asleep past the appointed
  moment settles the day whenever it wakes and that becomes the new anchor; left
  alone the changeover walks right around the clock. A *countdown* cannot survive a
  closed lid at all, which is why the tray's `TICK` decides only how often to ask the
  question, and `wake::watch` exists to ask it the moment the lid opens: `Instant`
  does not advance while the machine sleeps, so nothing monotonic can be trusted to
  notice midnight. Every deadline in `tray.rs` is therefore wall-clock seconds—
  `cooling_off` included.
- **Every failure path must set `cooling_off`.** The day is marked done only on
  success, so an error with no cooling-off period retries instantly and forever.
  In the tray that is `Schedule::failed`.
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
  human. `settings.json` is written by the window.** Three files because they have
  three authors — serialising config back would destroy the user's comments. The
  `source` string is decoded into `SourceSpec` while the file is read, so nothing
  downstream ever handles it as text. `settings.json` holds the filters and the
  placement style; it is written only by *Apply changes*, and a missing or
  unparseable one means the defaults, which are the program as it was before the
  window existed (landscapes, any shape, fitted over black).
- **Everything the program does is in the journal, and `journal` is the only
  writer.** One file — `Paths::log`, `~/Library/Logs/ArtWindow.log` on macOS, which
  is also where the launch agent sends stderr, so there is one file however the app
  was started. `journal::note!` for a fact, `journal::fault` for a failure with its
  causes; no `eprintln!` outside the command-line modes' own reports. Successes are
  recorded as well as failures, because a fault is found by the ordinary line before
  it — but only at a decision or an outcome, never per tick, per poll or per
  catalogue row. Every Dock restart names its reason, and every pin leaves the
  store's health. The file is appended to and cut back in place, never renamed,
  because launchd holds it open. The topics are listed in
  `docs/GUIDE.md#the-journal`.
- **Config, state and cache are different data kinds.** `Paths::locate` uses the
  platform directories for each instead of putting them under one convenient
  root. This preserves the existing Application Support/Cache split on macOS and
  maps to XDG config/data/cache on Linux. Favourites belong with durable state,
  never in the disposable cache.
- **GNOME wallpaper writes require the real session backend.** The Linux backend
  checks the schema before constructing `gio::Settings`, refuses to write without
  `DBUS_SESSION_BUS_ADDRESS`, sets both light and optional dark URIs, clears a URI
  before re-applying the same value, calls `sync`, and reads the result back. See
  `docs/gnome-wallpaper.md` before simplifying any of those steps.
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
  `fetching`. `tray::Schedule` holds that state — `fetching`, `cooling_off`,
  `superseded`, `asked_for_next` — and its `step` is the one question the tail
  asks; the arms above only tell it what happened. So *Next picture* raises
  `asked_for_next` and the tail reads it, jumping both the cooling-off period and
  the schedule, and a request made while a download is in the air is spent rather
  than kept for later. The row is greyed while a
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
  menu and the wake notification use. `Pick` has four variants — show, forget,
  apply, read — because selecting a picture, and trying out settings before
  applying them, change nothing outside the window and so have no business
  leaving it. *Read more* about a painter does leave it: starting a browser is
  something happening outside, so it is the loop's even though the window could
  do it.
- **No filter set may leave fewer than `museums::MIN_POOL` paintings.** Twenty. A
  choice counted as available when it matched one painting, and Africa plus
  Oceania matched two, which is the same two pictures in turn for ever. The
  catalogue owns the number and the search for what is in the way (`blockers`);
  `Pending` only words the answer, and *Apply* is refused below it. `Museums::fetch`
  asks the same question of whatever it is handed and relaxes the fewest sections
  that cure it, because the window is not the only way to arrive short: a
  `settings.json` from before the floor, a catalogue that shrank, *Screen-shaped*
  on a different display. `catalogue/build.py` keeps the same number as
  `MIN_PER_CHOICE` and leaves the `artist` column empty for a painter with fewer
  rows, so no app grows a chip with three paintings behind it. Android and iOS keep
  their own copies — `Catalogue.MIN_POOL` and `widened` in `Catalogue.kt`,
  `Catalogue.minPool` and `widened` in `Catalogue.swift` — duplicated on purpose
  like the word lists. They hide a thin choice where the desktop greys it.
- **A chosen painter wins over Shape and Origin, and over the floor.** Asking for
  a painter is asking for their paintings as they are: with any artist chosen,
  region and shape are not asked at all, and one painting is enough. Subject and
  *Hide religious scenes* still narrow. The floor exists for the combination
  nobody knew was thin; a painter's seventeen paintings in turn are what was
  asked for. Before this a phone set to phone-shaped paintings from Europe could
  choose nobody — six of the named painters' paintings are that shape, and twelve
  of the fourteen painters are from elsewhere. The threshold belongs to the
  filters being tested, so relaxing the Artist section brings twenty back, and
  `widened` never widens a painter away while they have something to show. All
  three copies of the rule say this; the windows show Shape and Origin as idle
  ("Not used while an artist is chosen.") rather than hiding them.
- **A phone may turn a wide painting; nothing else may.** *Turn wide paintings*
  (`rotateWide`, Android and iPhone, off by default) turns a painting wider than
  tall 90° clockwise before it is hung on a screen taller than wide, and every
  style then works on the turned picture. One helper per platform answers "the
  size as it will be hung", and the shape filter and the enlargement check both
  ask it, so the pool is judged as the paintings will hang. The renderer gates on
  a portrait screen itself, so a stale preference cannot turn a picture on a TV
  or an iPad's square canvas. The in-app picture, thumbnails and the iOS widget
  stay upright.
- **Framing is numbers, and one geometry reads them.** Pinch and drag on a
  phone's Settings preview — drag and scroll on the desktop's — set a zoom (1 to
  3, a style option that outlives the painting) and a pan per axis (0 to 1,
  belonging to the one painting named beside it, so the next painting is centred
  with nobody resetting anything). `Screen.frame` on Android, `Framing.rect` on
  iOS and `placement::frame` on the desktop place the painting for the renderer
  at screen size and for the preview at its own, which is the only reason the two
  agree; the three are copies kept in step by the same test cases, like the word
  lists. Zoom, Borders and Blur behind the picture can be framed; Stretch and a
  blur of the whole picture have no sharp picture to move. Android ignores
  framing on a landscape screen, which is its TV; the desktop is always one and
  frames regardless. On the desktop the gestures are `Pending::drag` and
  `zoom_about`, the pan is read only through `Framing::for_painting`, and a
  framed painting is composed like a blur — see the pixel rule below. On a
  phone the preview is a static backdrop with the sharp painting drawn over it,
  so a gesture redraws and never re-renders; the desktop re-renders a small
  copy, with the blurred backdrop kept between steps. Three things were learnt the
  hard way on Android: re-rendering the preview per touch event lurches; starting
  each event from the position rounded to a pixel loses a slow drag in one
  direction; and the event that lifts the last finger has an unspecified centroid,
  whose NaN, once in the pan, pins the painting to its left edge for good —
  `coerceIn` passes a NaN straight through. Admission (`canRender`, the shape
  filter) is always judged at zoom 1.
- **Android's colours are roles in `Theme.kt`, and none of them has a hue.** The
  app wore Material's default purple wherever a scheme was not given one. Every
  role of the scheme is now set, to match the desktop's system-colour roles, and
  screens name a role rather than a colour; what is left as a literal is data —
  the border picker's swatches, colours measured from a painting.
- **The settings tab decides nothing itself.** `gallery::Pending` owns what is
  staged, which chips show, what the preview looks like and whether *Apply* can
  be pressed; the three platform windows only draw it and forward clicks into it.
  A rule written into one platform file is a rule the other two will not have.
- **One painter at a time, and the row says who.** The *Artist* row is two chips
  with exactly one on: *Any artist*, and a second that carries the chosen
  painter's name and opens the browser. Choosing another painter replaces the
  first; nothing is ever taken out, so the browser's button reads *Choose* or
  *Chosen* and never *Remove*. Several at once came out as a mixture nobody had
  asked for by name. `filters.artists` is still a list on disk so no file had to
  migrate; a file holding several is read as its first.
- **A painter is chosen by a painting, not by a name.** The second chip opens a
  browser inside the
  settings tab that is the favourites browser again — the same shelf and preview,
  told different words for its two buttons. `Pending::artist_cards` hands each
  painter over as an `Artwork` precisely so that nothing about showing one had to
  be written a second time; that `Artwork` is for drawing only, and the card's
  `name` is who was clicked. The pictures are `catalogue/dist/artists/`, written by
  `catalogue/showcase.py` from the `showcase` and `about` in `artists.json` and
  compiled in by `build.rs`; `art::artists::picture` unpacks one into the cache as
  `artist-{slug}.jpg`, because every window makes thumbnails from a path. That
  prefix is neither source's, so no sweep deletes it. Android and iOS have
  the same row and browser, each modelled on its own favourites screen and each
  with its own copy of the rules, reading the same directory as bundled assets.
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
- **Rotation never decodes image pixels; only `placement` does.** `Artwork`
  carries a `PathBuf` and a download goes to disk undecoded. `placement::resolve`,
  called from `desktop::pin`, translates the style into the OS's own placement —
  fit plus a margin colour, fill, stretch — which is also what keeps mismatched
  monitors right, since each display places the picture itself. It reads pixels
  for exactly three things: the edge colour of *Automatic* borders; *Blur*, the
  one style no desktop offers; and a painting framed away from where its style
  would centre it, since a desktop can crop to the middle but cannot be told
  which part to show. The last two are composed at the main display's size into
  `rendered-{a,b}.jpg` in the cache (alternating, because macOS caches by path;
  the other is never deleted, because a Space waiting on a redraw may still name
  it). `placement::Preview` draws the settings tab's preview through the same
  geometry, but with a sampler of its own over halved copies of the painting,
  because it is redrawn for every step of a drag. The release profile is
  `opt-level = "z"`, and the `image` crate's generic resizing is compiled into
  this crate at that level whatever the crate itself is built with: a tenth of a
  second a frame, measured. `image` alone is built at `opt-level = 3`, which is
  what composing a wallpaper on *Apply* runs on.
  Galleries decode thumbnails through AppKit, GdkPixbuf or the shell and keep one
  full preview plus keyed thumbnails. The panel glyph is still ASCII art in
  `tray.rs`.
- **A day is a local day, and the OS is asked what that means.** `day::local` is the
  whole calendar this program has: one number per instant, comparison the only
  operation. The UTC offset comes from `NSTimeZone` on macOS and `GTimeZone` on
  Linux for *the instant in question* rather than for now, so the hours either side
  of a daylight-saving change do not read as the wrong day. No date crate—the OS
  already has the applicable timezone database.
- **`config.toml` may name settings that no longer exist.** `Config` has
  `deny_unknown_fields` so a typo is an error rather than a silent shrug, which means
  a retired setting cannot simply be deleted: every file written by an earlier
  version would fail to parse, and it would fail before there is a menu bar to say so
  in. `refresh_hours` is therefore still a field, typed `IgnoredAny`. Retiring a
  setting means moving it to that, not removing the line.
- **`Cargo.toml`'s `version` is the only version there is.** The release workflow
  refuses a tag that disagrees with it, `macos/bundle.sh` writes it over both keys
  in the bundle's `Info.plist` — whose own values are a template and nothing else —
  and CI hands it to Gradle as `-PappVersion`, where the Android `versionCode` is
  derived as `major*10000 + minor*100 + patch`. Editing any of the others by hand
  puts two versions in one tree. Releasing, and the keystore that must outlive it,
  are in **`docs/GUIDE.md#releasing`**.
- **`linux/install.sh` has to run from two layouts.** The release tarball holds it
  beside a prebuilt binary; a checkout has no binary and builds one. So it looks
  for a sibling `art-window` first, and every path it reads is resolved from its
  own directory rather than from the repository root. Reaching for `$root/linux/…`
  again breaks installing from a tarball, and nothing in the repo would notice.
- **A meeting app's slot always holds a painting no meeting has shown, and one
  meeting shows one painting.** `backdrop` overwrites the custom background a
  meeting app already knows — the Zoom client's, and Google Meet's in Firefox. A
  painting is *shown* once it has been read since it was written (access time
  later than modification time, which hanging arms by setting them that way
  round) while a meeting is live, and it is replaced when that meeting ends —
  never while it runs. Zoom reads the file twice per meeting, for the preview and
  again on joining, and a swap between the two opens the meeting with a different
  painting from the one previewed; that was the first version. All of this was
  observed rather than documented. Read **`docs/meeting-backdrops.md`** before
  touching it.
- **A Meet background must be exactly as long as the file it replaces.** Firefox
  records each blob's length in a database that cannot be edited while it runs,
  so the painting is a JPEG fitted under that length and padded to it with `COM`
  segments straight after SOI. The length of whatever the user uploaded is the
  budget for every painting after it.
- **Reading a slot file to look at it trips the wire.** `cp`, `file` or an image
  viewer moves the access time exactly as the meeting app does. `stat` does not.
- **A meeting background takes neither the desktop nor the day, and has a cache
  of its own.** `backdrop` hands the sources `cache/backdrop/<app>/` rather than
  the cache, so its downloads are never the wallpaper sweep's to delete and its
  own sweep can never reach today's painting. Each app runs on one thread with
  its own wall-clock cooling-off; the tray starts and stops them and schedules
  nothing for them. `state.backdrops` holds one slot per enabled app and is also
  the on/off switch — there is no second flag.
- **A meeting app owns everything about itself; the worker branches on nothing.**
  Where the background lives, what form it takes and what "live" means are a
  `Stage`, one per submodule of `backdrop`. `App::all` and `App::label` are all
  the tray knows, so a third app should touch `backdrop/` and nothing else.

## Deliberate omissions

Reversing these needs a reason, not a tidy-up impulse.

- **No dim effect.** The app this replaces had blur and dim; its user never enabled
  either. Blur came back in 2026-09 as one of the placement styles the phone apps
  already had, drawn by `placement` — dim did not, and there is no call for it.
- **Fit over black stays the default.** Filters, shape and placement styles
  arrived on the desktop from the phone apps, but every default in
  `settings.rs` reproduces the old behaviour, so nobody who never opens the
  settings tab sees anything change. The shape filter reads the pixel size the
  catalogue already verified and never decodes anything to measure it.
- **Not the Art Institute of Chicago.** Its metadata API is fine, but the image host
  `www.artic.edu/iiif/...` sits behind a Cloudflare managed challenge that an
  unattended client cannot answer. The Met has no such gate. Do not switch back.
- **Not Wikimedia Commons, and no source without its own commercial grant.** The
  apps are sold, so a source is admitted only if the institution's own published
  terms allow commercial use of its images. Commons warrants nothing per file —
  its "public domain" means the United States and the painting's country — and was
  dropped in 2026-10 for that reason, taking Oceania, South America and Africa out
  of the Origin filter with it; no clearly licensed source for those regions was
  found. The three apps still recognise the code `wmc`, because a Commons picture
  already on someone's desktop is theirs by its file name. Also looked at and
  refused: Nationalmuseum (Sweden), whose image server caps downloads at 1000 px;
  Yale, Te Papa, Brazil's federal museums and the Australian state galleries, on
  their terms. The quoted terms are in `docs/research/legal_check.md`.

## External services

The Met's API needs a real `User-Agent`; anonymous traffic is what earns bot
challenges. One request per day. `art/met.rs` filters to department 11 (European
Paintings) — an unfiltered collection of 490,000 objects is mostly coins and
textiles.

The search asks that department for `q=landscape` rather than `q=painting`, because
a generic query there comes back mostly portraits. `met.rs` never re-checks a
candidate's title or tags for "landscape" itself — the live search already
narrowed to it — only for whether the candidate reads as a portrait despite
matching, which is skipped for the next of the eight. The `met` source ignores
the settings tab's filters; only the `museums` catalogue can answer them.

`catalogue/build.py` draws from six museums instead of one: the Met, the
National Gallery of Art (Washington), the Cleveland Museum of Art, SMK
(Denmark), the Rijksmuseum and the Getty — all public domain or CC0, all
reachable over plain HTTPS with no auth. Only the Met sits behind Imperva, which throttles an unfamiliar client to
roughly 80 requests a minute; `catalogue/http.py`'s `PacedClient` is the one
place that paces every host (2 s between requests to the Met's
`collectionapi`, 1 s to the rest) and backs off on a 403 or 429, so no source
module has to remember any of that itself. NGA and Cleveland need no
per-object request at all — their open data already carries everything a row
needs — which is why only the Met pass takes hours. SMK's search deliberately
omits `lang=en`: passing it makes the API match nothing, so its titles and
tags come back in Danish — see **Android** below and `docs/android.md`. The
Rijksmuseum is read through its OAI-PMH endpoint, fifty records a request, rather
than its Linked Art documents at five requests a painting; the one request left
per painting is the IIIF `info.json` for the true pixel size, so its first pass
takes about an hour and a half. About one title in ten there has no English form
and stays Dutch. The Getty takes two requests a painting and gates on the image
document's rights, not the object's.

**The painter comes from the museum's own record, and a curated list decides who
gets a chip.** Every source yields the primary maker's plain name (`text.maker`
refuses workshops, followers and the unidentified); `catalogue/artists.json` lists
the painters worth a chip, with the spellings each museum uses as `aliases`, and
`build.py` fills the twelfth `artist` column only for them. The list is curated
because museums name hundreds of makers nobody would pick and spell one painter
several ways. The raw names survive a partial run in `catalogue/makers.tsv`, which
sits outside `dist/` because Android bundles all of `dist/`.

## The resident app

`tao` + `tray-icon` + `muda`, which pin the same `objc2 0.6` / `objc2-*-0.3` family
this project already used, so there is one copy of AppKit in the tree. `tray-icon`
needs a **GTK** event loop on Linux, which is why `tao` is the pairing and `winit`
is not.

Platform facts that are not guessable from the docs:

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
- Linux gives tao the application id `dev.artwindow`. `GApplication` owns
  uniqueness: a second ordinary launch activates the primary window and exits.
  The `quit` action is exported at `/dev/artwindow`, which is what `--quit` calls;
  changing the id, desktop filename, StartupWMClass or object path independently
  breaks that chain.
- A Linux panel icon exists only when an AppIndicator library and
  `org.kde.StatusNotifierWatcher` both exist. The D-Bus owner is watched at runtime.
  Missing either is ordinary stock-GNOME behavior, not a fatal error: the combined
  GTK window remains the complete interface.
- tao's GTK backend does not honor `ControlFlow::WaitUntil` by itself. One GLib
  timeout source is installed on the first `Init`; it only wakes the loop to ask the
  wall-clock question and never decides that a picture is due.

**The machine waking is a notification, not something to poll for.** macOS uses
`NSWorkspaceDidWakeNotification` from `NSWorkspace`'s own notification centre—the
default `NSNotificationCenter` never sees it. Linux uses logind's
`PrepareForSleep(false)` system-bus signal. `wake::watch` forwards either to the
loop through the same `EventLoopProxy` the menu uses. Its match arm asks for
nothing: the clock at the tail is re-read after *every* event, so arriving is very
nearly the entire message. The one thing it does say is that the screen is coming
back, which makes it a free moment to restart the Dock for any Space still waiting
on a picture — see `desktop::catch_up`. A failed logind subscription is nonfatal
because the GLib timer is the backstop.

**So is a display being unplugged.** On macOS the departing monitor's Spaces land
on the remaining screens showing the Dock's default picture. `wake::displays`
forwards `NSApplicationDidChangeScreenParametersNotification` (default centre, not
the workspace's), and its arm owes an asking `SETTLE` seconds later — the same debt
beginning a session owes — without counting as a redraw, since the user is looking
at the screen. See `docs/macos-wallpaper.md#unplugging-a-display`.

**The favourites window is tao's, and only what is inside it is AppKit's.** A
`WindowBuilder` buys the title bar, the close button arriving as
`WindowEvent::CloseRequested`, resizing, and `Window::set_focus()` — which already
does `makeKeyAndOrderFront:` followed by `activateIgnoringOtherApps:`, the pair an
`Accessory` app needs to put a window in front of anything. `gallery/macos.rs`
reaches through `WindowExtMacOS::ns_view()` and fills it, and lays its own container
inside that so the arithmetic is written in coordinates this program decides the
orientation of rather than tao's. Everything resizes by autoresizing mask, so
`WindowEvent::Resized` never has to be handled at all.

Closing a window is not quitting, and its arm deliberately does not `return`—the
clock at the tail still has to be wound. macOS drops its optional gallery. Linux
minimises its one combined window so the GApplication can reactivate the same
surface; when no panel indicator exists, another launcher activation brings it
back.

**Start at login is a file-exists setting.** On macOS it is a launchd agent rather
than `SMAppService`, because the development machine runs 12.7. On Linux it is
`$XDG_CONFIG_HOME/autostart/dev.artwindow.desktop`, with the current executable
quoted under the desktop-entry and Exec grammars. Neither backend bootstraps,
starts, or stops anything; the setting only means something at the next login.

## Android

A native Kotlin app under `android/`, alongside `linux/` and `macos/`, sharing no
code with the Rust side. Full detail, including the shape filter's numbers and the
placement and scheduling design, is in `docs/android.md`. The invariants that
matter across the boundary:

- **`Screen` owns every hanging rule.** `MAX_TRIM`, `MAX_ENLARGEMENT` and the
  catalogue-measurement slack all live in `Screen.kt`; nothing else on the Android
  side decides whether a painting fits.
- **`Wallpaper.pin` owns placement**, the same way `desktop::pin` does on the
  desktop: it composes the bitmap at exactly the screen's size before handing it
  to `WallpaperManager`, so callers never crop, scale or position anything
  themselves. On a TV there is no wallpaper, so the same call publishes the
  rendered bitmap to `Frame` instead; the screensaver (`ArtDream`) and art mode
  (`TvActivity`) only observe `Frame.shown` and never render or place anything.
- **`Screen` keeps the orientation the display reports** — portrait on a phone,
  landscape on a TV — and `ArtworkShape.SCREEN` (stored as `PHONE` by older
  builds, which `shapeValue` still reads) means "shaped like this screen".
- **Only `Rotation.turn` fetches, and only one turn at a time.** It takes a
  `tryLock` rather than queuing a second attempt behind the first.
- **The day is a calendar comparison, never a countdown.** `State.isDue` compares
  `LocalDate` epoch days — the same rule as the desktop's `is_due`, including the
  05:00 start: every caller asks `Day.today()` rather than `LocalDate.now()`. iOS
  keeps the same hour in `Day.beginsAtHour`, and its Shortcuts automation runs at
  05:05 for that reason.
- **The list is generated, never hand-edited, and applies only objective gates.**
  `catalogue/build.py` — a separate, offline pipeline, see **External services**
  above — walks the Met, the National Gallery of Art (Washington), the Cleveland
  Museum of Art and SMK (Denmark) and writes `catalogue/dist/paintings.tsv`:
  checked in, compiled straight into the desktop binary and bundled as an Android
  asset. It keeps only what is public domain or CC0, catalogued as a painting, has
  a direct JPEG, is at least 2000 px on its long side, and names a known region —
  nothing about subject, portrait, religious content or shape, and the pixel size
  it records is verified at build time rather than trusted from catalogue metadata.
- **Selection rules are duplicated on purpose, like the Met protocol used to be.**
  Subject matching — including the Danish words SMK's Danish-language records need
  — the portrait exclusion, the religious-scene list and the shape limits live in
  `Catalogue.kt` on Android, in `src/art/museums.rs` and `src/settings.rs` on the
  desktop, and in `Catalogue.swift` on iOS. Rather than one side calling into the
  other, each keeps its own copy, so a change to a word list has to be made in
  every file on purpose.
- **A downloaded file's prefix says who owns it.** The desktop writes
  `museums-{source}-{id}.{ext}`; the distinct `museums-` prefix is load-bearing —
  it is what keeps this source's `key_of` and the older `met` source's `id_of`
  from ever recognising each other's downloads as their own to delete. Android
  instead keys a file by `{source}-{id}`, with a legacy `met-{id}` name (from
  before the four-museum catalogue existed) still parsing the same way, since
  `"met"` remains one of its recognised museum codes.

## iOS

A native Swift app under `ios/` (iOS 17, iPhone and iPad), a port of the Android
app sharing only `catalogue/dist/paintings.tsv`. Full detail in `docs/ios.md`.

- **iOS cannot set the wallpaper.** The app exposes `GetTodaysPaintingIntent`,
  which returns the painting rendered at the wallpaper's pixel size, and a Shortcuts
  personal automation feeds it to *Set Wallpaper*. Nothing may call private
  wallpaper API.
- **The intent never fails on the network.** A download that fails or runs past
  its budget returns the painting already shown. It still cools off, and the day
  stays owed.
- **Widgets observe, never fetch.** `Rotation` writes a downsampled `widget.jpg`
  and reloads the timelines. The widget reads nothing larger.
- **App, intents and widget share the App Group `group.dev.artwindow`**
  (`SharedContainer`), so signing needs a paid team.
- **On iPad the canvas is square**, the long side on both axes, because one
  wallpaper serves both orientations. `DeviceScreen` owns that, and remembers the
  value for callers off the main thread.
- **The word lists live three times now**: `Catalogue.kt`, `src/art/museums.rs`
  and `ios/ArtWindowKit/Catalogue.swift`.
- **Builds run on `ios_macmini`**: the laptop's Xcode 14.2 can't target iOS 17.
  `ios/project.yml` is the source and the `.xcodeproj` is generated, not
  checked in.

## Windows

The third desktop backend, behind the same four seams: `desktop/windows/`,
`gallery/windows.rs`, and a Windows body in `day.rs` and `wake.rs`. Rotation and
state still do not branch on the operating system. It has been verified by CI
only; see `docs/windows-wallpaper.md` for what is still unwatched.

- **Strip `\\?\` before the shell sees a path.** `desktop::pin` canonicalises,
  and on Windows that yields a verbatim path that `IDesktopWallpaper` and
  `IShellItemImageFactory` reject. Both Windows files strip it themselves, so
  the seam stays unchanged.
- **One instance per session is a named mutex, `Local\dev.artwindow`**, taken in
  `tray::run` through `desktop::claim_instance`. Autostart plus a Start-menu click
  would otherwise make two tray icons. `--quit` signals the named event
  `Local\dev.artwindow.quit` the claim listens on — Windows' version of the
  GApplication `quit` action.
- **The `Run` value is shared with the installer.** `windows/art-window.iss`'s
  sign-in task and the tray's *Start at login* row both write
  `HKCU\…\CurrentVersion\Run\ArtWindow`. The install is per-user
  (`PrivilegesRequired=lowest`) precisely so both live in HKCU. Rename one and the
  row stops telling the truth.
- **`wake::watch` takes `Send + Sync`.** Windows delivers resume on a system
  thread; the closure only posts to the `EventLoopProxy`, which is what makes
  that safe everywhere.
- **Release builds are GUI-subsystem.** No console appears behind the tray. The
  command-line modes call `desktop::attach_console` to print to the terminal that
  started them, and the resident app sends stderr to `art-window.log` beside
  `state.json` (`desktop::log_to`).
- The build embeds `windows/art-window.manifest` (Common Controls v6, which
  `SetWindowSubclass` needs, and PerMonitorV2 DPI) and `windows/art-window.ico`.

## The app icon

`icon/icon.svg` is the only drawing. `./icon/make.sh` derives every platform's
icon from it — macOS `AppIcon.icns`, the Linux SVG, the Windows `.ico`, the iOS
1024 PNG, Android's adaptive foreground, background colour and TV banner — and the
outputs are checked in, so no build needs it. Edit the SVG and rerun the script;
never touch a derived file by hand. The menu-bar glyph is separate: ASCII art in
`tray.rs`.

## The website

`site/` is the landing page at artwindow.alps-project.online. **Its Download
buttons download nothing.** Every one of them, on the comparison pages too, opens
the `#get` sheet, which asks for an email and promises a link to install the app;
iPhone & iPad is one more platform in the same form. `subscribe.php` appends the
address, the date and the platform asked for to a CSV outside the web root, and
nothing else is stored. Nothing sends the links: they go out by hand from that
file. No page links to a release file, and putting one back ends the test the
sheet exists for. It needs a PHP host; see `site/HOSTING.md`.
The privacy policy is the `#privacy` sheet at the bottom of `index.html`, and it
promises no cookies, analytics, third-party scripts or services, and apps that
send nothing but the painting download. Adding any of those means changing the
policy and its date in the same commit.

**The site states a price the apps do not enforce yet.** Since 2026-10-01 the model
is paid: the daily painting is free, and one licence — $14.99 once, for every
device — unlocks the settings tab's filters and placement styles. Favourites stay
free. No app checks a licence and there is no checkout, which is why the page says
"Licences go on sale soon" rather than carrying a Buy button. A checkout or a
licence check is a third-party service or something the apps send, so either one
changes the privacy policy too. The page no longer links to the repository or the
guide. Promo material (`promo/reel.html`, `promo/scripts/`) says neither "free" nor
a price. The `docs/2026-09-29_AG_*` documents still argue the free model they were
written under.
