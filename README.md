# Art Window

A daily painting on your desktop, always fit to the screen with black borders.

Art Window fetches a public-domain painting once a day and sets it as your
wallpaper—scaled to fit entirely on screen, never cropped, with the margins filled
black. A portrait painting reads as a framed picture on a black wall.

Inspired by [Muzei](https://github.com/romannurik/muzei) by Roman Nurik and its
[macOS port](https://github.com/naman14/Muzei-macOS) by Naman Dwivedi. This is an
independent rewrite and shares no code with either.

## Status

Art Window supports macOS and Linux with GNOME. The GNOME port uses GTK 3,
GSettings, logind, and the XDG directory conventions. Windows is not implemented.

## Install

The build requires Rust 1.88 or newer.

### macOS

```sh
./macos/bundle.sh
cp -R "target/Art Window.app" /Applications/
```

Open the app to put its framed-picture icon in the menu bar.

### Linux/GNOME

Install GTK and D-Bus development files, then run the user-local installer:

```sh
# Debian or Ubuntu
sudo apt install build-essential pkg-config libgtk-3-dev libdbus-1-dev

# Arch Linux
sudo pacman -S --needed base-devel pkgconf gtk3 dbus rust
```

Then run `./linux/install.sh`.

On NixOS, build and install from a temporary development shell instead:

```sh
nix-shell -p rustc cargo pkg-config gtk3 dbus \
  --run './linux/install.sh'
```

This installs the binary under `~/.local/bin` and a GNOME launcher and icon under
`~/.local/share`. Set `ART_WINDOW_PREFIX` or `XDG_DATA_HOME` before running the
script to override those locations.

The GTK window is the complete interface on stock GNOME. If an AppIndicator
library and a StatusNotifier extension are available, Art Window also adds a panel
menu and can stay out of the way there. Those are optional; their absence never
makes the app unusable. See [GNOME wallpaper integration](docs/gnome-wallpaper.md)
for the exact behavior and diagnostic commands.

## Interface

On macOS, the menu is the primary interface. On GNOME, the same actions appear in
one GTK window with the favourites browser below them; the optional panel menu is a
compact second surface.

```text
L'Arlésienne: Madame Joseph-Michel Ginoux
Vincent van Gogh, 1888–89
Open in browser
─────────────────────
Next picture
Add to favourites
Favourites…
Back to today's picture
─────────────────────
Re-apply wallpaper
✓ Start at login
─────────────────────
Quit Art Window
```

**Next picture** fetches another painting immediately. It is the day's rotation
asked for early rather than a separate thing: the painting that arrives is today's,
the one it replaces is gone, and tomorrow's still comes with tomorrow.

**Add to favourites** copies the painting on the desktop into safe storage. The
ordinary cache holds one picture, so this is the only action that saves it from the
next rotation.

**Favourites…** opens the macOS favourites window. On GNOME that browser is already
part of the main window. Pictures run down the left; selecting one loads its larger
preview on the right.

```text
┌────────┬─────────────────────────────┐
│ ┌────┐ │      ┌───────────────┐      │
│ │    │ │      │               │      │
│ └────┘ │      │               │      │
│ ┌────┐ │      │               │      │
│ │    │ │      │               │      │
│ └────┘ │      └───────────────┘      │
│        │  Sahurs Meadows in Morning… │
│        │  Alfred Sisley, 1894        │
│        │  [Set as wallpaper] [Forget]│
└────────┴─────────────────────────────┘
```

**Set as wallpaper** puts a kept painting up—a double-click does the same—and
**Back to today's picture** restores the rotation's painting. **Forget** removes a
painting from the list; if it is currently on the desktop, its file waits until the
desktop has moved on.

Choosing an existing painting by hand does not disturb the schedule. The exception
is a painting that was already overdue: that choice settles the day, since
otherwise an overdue fetch would immediately replace it.

**Start at login** writes a launchd agent on macOS or an XDG autostart entry on
Linux. It takes effect at the next login; changing it neither starts nor stops the
current process.

Art Window watches the local date rather than a stopwatch. A machine that sleeps
through several days wakes owing one painting, not one per missed day. macOS and
Linux both subscribe to their native wake notifications and also retain a timer as
a backstop.

## Use

Art Window remains useful as a command:

```sh
art-window            # run the resident app
art-window --once     # fetch a painting now, print it, then exit
art-window --if-due   # the same, but only if the local day is unsettled
art-window --where    # print config, state, cache and favourites locations
art-window --check    # diagnose GNOME integration (Linux only)
art-window --quit     # stop the running GNOME instance (Linux only)
```

Launching the GNOME app a second time brings the existing window forward instead
of starting another rotation process.

The macOS binary lives inside the bundle. Link it onto your path if you want the
one-shot commands:

```sh
mkdir -p ~/.local/bin
ln -s "/Applications/Art Window.app/Contents/MacOS/art-window" ~/.local/bin/
```

## Settings

`config.toml` lives at the location `--where` reports. It is read and never
rewritten, so comments survive.

```toml
# "met" for public-domain paintings from the Metropolitan Museum,
# or a path to a folder of your own pictures.
source = "met"
```

One painting a day is the whole schedule and there is nothing to tune. A
`refresh_hours` left over from an older version is accepted and ignored so an
existing config keeps working.

Pointing `source` at a folder inside `~/Pictures` or `~/Documents` can make macOS
ask for access. A launchd process cannot show that prompt, so run
`art-window --once` from a terminal to approve it.

Settings are read when Art Window starts. After editing `config.toml`, quit and
reopen it.

## Uninstall

Turn off **Start at login** and quit Art Window first.

On macOS:

```sh
rm -rf "/Applications/Art Window.app"
rm -rf ~/Library/Application\ Support/ArtWindow
```

On Linux, for the default installer locations:

```sh
rm -f ~/.local/bin/art-window
rm -f ~/.local/share/applications/dev.artwindow.desktop
rm -f ~/.local/share/icons/hicolor/scalable/apps/dev.artwindow.svg
rm -f ~/.config/autostart/dev.artwindow.desktop
rm -rf ~/.config/artwindow ~/.local/share/artwindow ~/.cache/artwindow
```

Adjust those paths if the installer or XDG directories were overridden. The
wallpaper stays as it is; choose another in system settings to change it back.

## Credits

Artwork metadata and images come from [The Metropolitan Museum of Art Collection
API](https://metmuseum.github.io/), under its open-access terms.
