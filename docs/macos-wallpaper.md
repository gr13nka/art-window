# Setting a wallpaper on macOS

Everything here was learned by getting it wrong first. The short version: the
obvious API works, reports success, and does not change the wallpaper the user is
looking at.

## Spaces are the whole problem

macOS stores a **separate wallpaper for every Mission Control Space, on every
display**. `NSWorkspace.setDesktopImageURL(_:for:options:)` writes only the Space
that is active *for the calling process* — and it returns success either way.

On the machine this was developed against: 23 Spaces, 3 registered displays, 49
space/display slots. A successful call updated **2 of 49**. Every automated check
reported success while the desktop visibly did not change, because the process was
launched from a terminal sitting on a different Space.

Apple exposes no public API for the other Spaces. The options are the Dock's private
store, undocumented CoreGraphics calls, or accepting one-Space-only. `desktop::pin`
uses the first, and keeps the supported call as well so there is something to fall
back to.

## The Dock's store

`~/Library/Application Support/Dock/desktoppicture.db`, SQLite.

```
pictures(space_id, display_id)              -- one row per slot; rowid = picture_id
preferences(key, data_id, picture_id)       -- settings, keyed by slot
data(value)                                 -- shared pool of values
```

Relevant `preferences.key` values:

| key | meaning |
|-----|---------|
| 1 | image path |
| 2 | placement — `5` is fit-with-letterbox |
| 3, 4, 5 | fill colour, one row per RGB channel |

Four things about this that will cost time if forgotten:

- **Paths are tilde-abbreviated.** The Dock stores `~/Library/...`, and matches only
  its own form. An absolute path silently fails to match.
- **`data` is type-sensitive.** Integer `5` and text `"5"` are different rows. Look
  values up with `typeof(value) = typeof(?)` or duplicates accumulate.
- **A trigger prunes orphans.** `preferences_deleted` removes `data` rows that lose
  their last referrer. Anything inserted *before* the delete can be swept away
  before it is pointed at, so **delete first, then insert**.
- **The Dock caches all of it in memory.** A write is invisible until `killall Dock`,
  which relaunches immediately and closes nothing. Restart only when the image
  actually changed, or the Dock blinks on every no-op.

## AppKit details

- `NSScreen::screens` requires a `MainThreadMarker`, so the macOS `desktop::pin`
  backend only works on the main thread. It raises an error rather than documenting
  the rule, because a scheduler thread calling it would otherwise fail in a
  confusing way.
- `NSImageScaling::ScaleProportionallyDown` is documented as **not supported** for
  desktop images. Use `ScaleProportionallyUpOrDown` with `allowClipping = false`;
  that pairing is what "fit" means.
- The fill colour must be convertible to `NSCalibratedRGBColorSpace`, and its alpha
  is ignored. `NSColor::black` lives in the calibrated **white** space, so build it
  with `colorWithCalibratedRed_green_blue_alpha` instead of relying on a conversion.

## Verifying a change actually landed

Do not trust the API's return value, and do not trust `System Events`, which reads
the same store rather than what is drawn. Read the slots:

```sh
sqlite3 ~/Library/Application\ Support/Dock/desktoppicture.db \
  "select substr(d.value,-24), count(*) from preferences p
   join data d on d.rowid = p.data_id where p.key = 1 group by d.value;"
```

Every slot should name the current image. To see what is actually on screen,
`screencapture -x` and look at it.

## At login the Dock is still building it

The Dock starts a few seconds before Art Window does and spends the first minute or
so of a session putting this database together. A write during that window is
refused — `database is locked`, sometimes `disk I/O error` — and refused *in both
directions*, because neither the Dock nor SQLite's default waits for the other's
lock.

The evidence, a minute after one login, in a store the Dock had just started over:
`displays` and `spaces` both empty, `pictures` holding one row naming neither, and
the Dock's own account of the same failure sitting in `prefs`:

```
5001|could not add new preferences - err=5 errmsg=database is locked
     loc=-[DPPictureStorage setDictionary:forDisplay:andSpace:displayID:updateKey:isDefault:]:479
```

So the Dock lost the wallpaper the supported `NSWorkspace` call had just set, and
this program lost the other Spaces. Twenty minutes later the same file took a
`BEGIN IMMEDIATE` without complaint: at login this is congestion, not damage, and
the answer is to wait a little and ask again rather than to conclude anything.

Both halves of that answer are in the code. `spread_to_every_space` gives the
connection a `DOCK_BUSY_WAIT` busy timeout, so a moment's overlap is waited out
instead of failing instantly. What it cannot wait out it reports as
`Pinned::InPart`, and `tray::Reassert` offers the picture again a minute later, five
times over. Nothing else would: the day is settled the moment a painting arrives,
so no rotation comes back to it until tomorrow — which is why beginning a session
owes one asking whatever the state file says.

## When the store is not there to be written

macOS can decide the database is bad, rename it to `desktoppicture.db.corrupt` and
start an empty one. It happened during a long session of testing, and it looks like
a bug in this program rather than what it is: writes fail with `database is locked`
or `disk I/O error`, `pin` reports that only the active Space was updated, and every
Space but the visible one keeps the old picture.

Nothing here needs fixing when that happens. `killall Dock` and the Dock repopulates
`displays`, `spaces` and `pictures` as Spaces are visited; once `SELECT count(*) FROM
pictures` is non-zero again, `spread_to_every_space` works exactly as before. Two
things worth knowing before reaching for the debugger: the quarantined file usually
passes `pragma integrity_check`, so its name is not evidence of what went wrong, and
an empty store is also the one case where `pin` is *supposed* to do nothing beyond
the active Space, because `slots == 0` returns early by design.
