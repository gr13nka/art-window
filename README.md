# Art Window

A daily painting on your desktop, always fit to the screen with black borders.

Art Window fetches a public-domain painting once a day and sets it as your
wallpaper — scaled to fit entirely on screen, never cropped, with the margins
filled black. A portrait painting reads as a framed picture on a black wall.

Inspired by [Muzei](https://github.com/romannurik/muzei) by Roman Nurik and its
[macOS port](https://github.com/naman14/Muzei-macOS) by Naman Dwivedi. This is an
independent rewrite and shares no code with either.

## Status

**macOS only.** Windows and Linux backends are planned but not written; the crate
does not currently build for them.

## Install

```sh
./macos/bundle.sh
cp -R "target/Art Window.app" /Applications/
```

Open it and a small framed-picture icon appears in the menu bar. That is the whole
interface:

```
L'Arlésienne: Madame Joseph-Michel Ginoux
Vincent van Gogh, 1888–89
Open in browser
─────────────────────
Add to favourites
Favourites            ▸   L'Arlésienne: Madame…  ▸  Show
                          Wheat Field with Cyp…     Forget
Back to today's picture
─────────────────────
Re-apply wallpaper
✓ Start at login
─────────────────────
Quit Art Window
```

**Add to favourites** takes a copy of the painting on the desktop and keeps it,
which is the only thing that saves it: the cache holds one picture, and tomorrow's
arrival deletes today's. **Favourites ▸ … ▸ Show** puts a kept painting up when the
day's does not suit, and the last row — which names the painting it means — puts
today's back when you have had enough of the substitute. **Forget** drops a painting
from the list; if it happens to be the one on the desktop, its file waits until the
desktop has moved on.

Choosing a painting by hand does not disturb the schedule: today's stays today's,
and tomorrow's arrives at its usual hour no matter how often you change your mind in
between. The exception is a painting that was already overdue — then whatever you
choose settles the day, since something had to.

Everything kept lives in one folder — `art-window --where` will say which — so the
whole collection is one thing to copy or throw away.

**Start at login** writes a launchd agent to `~/Library/LaunchAgents`. It takes
effect at your next login — switching it on cannot start a program that is already
running, and switching it off should not stop one.

While it runs, Art Window checks the clock every few minutes and fetches a new
painting once the current one has had its `refresh_hours`. It compares wall-clock
times rather than counting down, so a laptop that spent the week shut wakes up owing
exactly one painting, not seven.

## Use

Art Window is a menu bar app, but it stays a command as well — useful when you want
to see what happens rather than trust that it did.

```sh
art-window            # live in the menu bar (what the .app does)
art-window --once     # fetch a painting now, print it, exit
art-window --if-due   # the same, but only if one is due
art-window --where    # print where settings, state, cache and favourites live
```

The binary lives inside the bundle, so either call it there or link it onto your
path:

```sh
ln -s "/Applications/Art Window.app/Contents/MacOS/art-window" ~/.local/bin/
```

## Settings

`config.toml`, in the directory `--where` reports. It is read and never written, so
your comments survive.

```toml
# "met" for public-domain paintings from the Metropolitan Museum,
# or a path to a folder of your own pictures.
source = "met"

# Hours a picture stays up before the next one is fetched.
refresh_hours = 24
```

Pointing `source` at a folder inside `~/Pictures` or `~/Documents` will make macOS
ask for access. A process started by launchd cannot show that prompt, so run
`art-window --once` from a terminal to approve it.

Settings are read when Art Window starts. After editing `config.toml`, quit and
reopen it.

## Uninstall

Turn off **Start at login**, quit from the menu, then:

```sh
rm -rf "/Applications/Art Window.app"
rm -rf ~/Library/Application\ Support/ArtWindow
```

Your wallpaper stays as it is. To change it back, pick another in System
Preferences.

## Credits

Artwork metadata and images from [The Metropolitan Museum of Art Collection
API](https://metmuseum.github.io/), used under its open-access terms.
