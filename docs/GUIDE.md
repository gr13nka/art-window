# Art Window guide

Everything the [README](../README.md) links out to: platform support, the full
install for each platform, every menu action, the command-line flags, the settings
file, and how to remove it.

## Contents

- [Status](#status)
- [Install](#install)
- [Interface](#interface)
- [Use](#use)
- [Settings](#settings)
- [Uninstall](#uninstall)
- [Releasing](#releasing)
- [Credits](#credits)

## Status

Art Window supports macOS, Linux with GNOME and Windows 10/11 as desktop wallpaper apps,
Android as a native Kotlin phone app, and iPhone and iPad as a native Swift app. The GNOME port uses GTK 3, GSettings,
logind, and the XDG directory conventions. The Windows port is built and tested
in CI but has not yet been tried on a real desktop — see
[Windows wallpaper integration](windows-wallpaper.md).

## Install

The build requires Rust 1.88 or newer.

### macOS

**From a release:** open the DMG and drag Art Window into Applications. The app
is ad-hoc signed rather than notarized, so the first launch needs right-click →
Open, or `xattr -dr com.apple.quarantine "/Applications/Art Window.app"`.
Universal binary, macOS 11 or newer.

**From source:**

```sh
./macos/install.sh
```

This rebuilds the current checkout and replaces `/Applications/Art Window.app`.
Set `ART_WINDOW_APP_DIR` to an absolute directory to install somewhere else.

Open the app to put its framed-picture icon in the menu bar.

### Linux/GNOME

**From a release:** needs GTK 3 at runtime — any GNOME desktop already has it —
and glibc 2.35 or newer (Ubuntu 22.04, Debian 12, or newer), x86_64 only.

```sh
tar -xzf art-window-*-linux-x86_64.tar.gz
cd art-window-*-linux-x86_64
./install.sh
```

**From source:** install GTK and D-Bus development files, then run the
user-local installer:

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
makes the app unusable. See [GNOME wallpaper integration](gnome-wallpaper.md)
for the exact behavior and diagnostic commands.

### Windows

**From a release:** run `art-window-windows-x64-setup.exe`. It installs for the
current user only, so it needs no administrator rights. It adds a Start-menu
entry and, unless you untick it, starts Art Window whenever you sign in. When
setup finishes, Art Window starts in the notification area and puts up the day's
painting. Windows 10 1809 or newer, x64.

**From source:** needs the MSVC toolchain and Inno Setup 6.

```powershell
./windows/package.ps1   # -> target/dist/Art-Window-<version>-windows-x64-setup.exe
```

The tray icon may land in the overflow flyout (the `^` by the clock). Drag it onto
the taskbar to keep it in sight.

### Android

**From a release:** download the APK on the phone, allow the browser to install
unknown apps, then open it. Android 11 or newer (minSdk 30). Releases are signed
with one key, so each installs over the last.

**From source:** needs the Android SDK plus JDK 17, and a phone with USB
debugging enabled. Then run `./android/install.sh` to build and install the
debug APK.

It replaces both the home and lock screen wallpaper, and — unlike the desktop —
fills the screen rather than letterboxing, picking only paintings tall enough for
that to look right. See [Art Window for Android](android.md) for why. The
painting changes on the first hourly check after 05:00, over Wi-Fi.

### iPhone and iPad

**From source only** for now: needs the build Mac (`ios_macmini`, Xcode 26), a
paid Apple Developer team set as `DEVELOPMENT_TEAM` there, and an iPhone or iPad
on iOS 17 or newer paired with it. Then run `./ios/remote.sh install`.

iOS won't let an app set the wallpaper, so Art Window hands the painting to
Shortcuts instead. On first launch, the app walks you through a daily automation:
Get Today's Painting, then Set Wallpaper. There are also home-screen and StandBy
widgets that need no setup. See [Art Window for iPhone and iPad](ios.md) for why.

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
Settings…
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

**Settings…** opens the same window on its *Settings* tab (on GNOME, the tab at
the top of the main window). On the left is the painting on your desktop, drawn
the way the choices on the right would hang it:

- **Style** — *Borders* fits the whole painting and fills the margins with black,
  a colour taken from the painting's edge, or a colour of your own. *Zoom*
  fills the screen and crops. *Stretch* fills it and distorts. *Blur* sets the
  painting over a blurred copy of itself, or shows only the blur.
  Drag the preview to choose which part of the painting shows, and scroll (or
  pinch on a trackpad) to zoom in up to three times. The zoom stays for every
  painting; the position is remembered for that one painting, so the next
  arrives centred. *Stretch*, and a blur with no picture over it, cannot be
  moved.
- **Shape** — *Screen-shaped* keeps paintings close to your main display's
  proportions; *Near square* also allows squarer ones.
- **Origin** and **Subject** (landscape, seascape, still life) — pick any number
  in each; nothing picked means any. A choice that would leave fewer than twenty
  paintings is greyed, and clicking it says which of the other choices is in the
  way.
- **Artist** — the row lists the painters you have chosen; click one to take it
  out. *Any artist* (or *Add*) opens a browser like the favourites one: each
  painter is shown by their best-known painting, with *Choose* and a *Read more*
  link to their Wikipedia article. *Back* returns to the settings. A chosen
  painter wins over **Shape** and **Origin**, which grey out while one is chosen,
  and the twenty-painting minimum does not apply to them; **Subject** and **Hide
  religious scenes** still narrow what arrives.
- **Hide religious scenes.**

**Apply changes** saves them, and stays greyed while fewer than twenty paintings
match — a smaller selection is the same few pictures coming round again. Filters
saved by an earlier version that fall short are widened by as little as it takes
when the next painting is picked, and the tab says so. A new style re-hangs the painting on the desktop at
once; filters take effect from the next painting. Filters need `source =
"museums"` — the Met's live search and a folder of your own pictures cannot be
filtered this way.

Choosing an existing painting by hand does not disturb the schedule. The exception
is a painting that was already overdue: that choice settles the day, since
otherwise an overdue fetch would immediately replace it.

**Start at login** writes a launchd agent on macOS, an XDG autostart entry on
Linux, or an `ArtWindow` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
on Windows — the same value the installer's sign-in option writes. It takes effect at the next login; changing it neither starts nor stops the
current process.

Art Window watches the local date rather than a stopwatch. A machine that sleeps
through several days wakes owing one painting, not one per missed day. macOS,
Linux and Windows all subscribe to their native wake notifications and also retain a timer as
a backstop.

## Use

Art Window remains useful as a command:

```sh
art-window            # run the resident app
art-window --once     # fetch a painting now, print it, then exit
art-window --if-due   # the same, but only if the local day is unsettled
art-window --where    # print config, state, cache, favourites and journal locations
art-window --check    # diagnose GNOME integration (Linux only)
art-window --quit     # stop the running instance (Linux and Windows)
art-window --catalogue  # count the paintings behind every filter combination
```

On Windows the one-shot commands print to the terminal they are run from.
The installed binary is `%LOCALAPPDATA%\Programs\Art Window\art-window.exe`.
Anything a GUI program would otherwise print to nowhere goes to the journal,
below.

Launching the GNOME app a second time brings the existing window forward instead
of starting another rotation process.

The macOS binary lives inside the bundle. Link it onto your path if you want the
one-shot commands:

```sh
mkdir -p ~/.local/bin
ln -s "/Applications/Art Window.app/Contents/MacOS/art-window" ~/.local/bin/
```

### The journal

Art Window writes down what it does as it does it: one file, one line per fact,
kept on your machine and sent nowhere.

| Platform | File |
|----------|------|
| macOS | `~/Library/Logs/ArtWindow.log` (Console shows it under *Log Reports*) |
| Linux | `art-window.log`, beside `state.json` |
| Windows | `art-window.log`, beside `state.json` |

`art-window --where` prints the exact path. A line looks like this:

```
2026-10-04 15:48:29.272 +0200 [pin] …
```

The time is local, with its offset from UTC, so it can be laid beside the
system's own logs. The word in brackets is the topic:

| Topic | What it records |
|-------|-----------------|
| `start` | the version, how the program was started, where its files are, the state it found |
| `loop` | what woke the program: a session beginning, the machine waking, a display changing, a click |
| `schedule` | why a download started, a cooling-off after a failure |
| `fetch` | each request to a museum, the painting chosen, why another was passed over |
| `pin` | hanging a picture: what was asked of the desktop and what it answered |
| `dock` | macOS only: the health of the Dock's wallpaper store, and every Dock restart with its reason |
| `owed` | what the desktop is still owed, and each time it is offered again |
| `state`, `settings`, `favourites`, `sweep` | every file written or deleted |
| `backdrop` | meeting backgrounds: a meeting beginning and ending, a painting hung |
| `desktop` | start at login, wake and display subscriptions |
| `panic` | the program stopping where it should not have |

A failure reads `FAILED:` followed by every cause behind it. To find one:

```sh
grep FAILED ~/Library/Logs/ArtWindow.log
```

and read upwards from it: the ordinary lines just before are usually the
explanation. The file is cut back to its last quarter-megabyte when it passes one
megabyte, at start-up, so it never needs clearing.

### A painting behind you in meetings

On macOS, Art Window can keep a painting as your virtual background in Zoom and
in Google Meet in Firefox, and change it for every meeting. It can only replace
a background the app already knows, so each needs one picture added by hand
first, and that picture is overwritten.

**Zoom**

1. In Zoom, open *Settings → Background & effects*, add any picture with **+**
   and select it.
2. Tick *Paintings in Zoom meetings* in the Art Window menu.

**Google Meet in Firefox**

1. In a Meet call in Firefox, open *Backgrounds and effects*, upload a picture
   as a custom background and select it. Upload a large one, a photo of a few
   megabytes: the paintings have to fit in the space it takes, and a small
   upload means coarser paintings.
2. Tick *Paintings in Google Meet (Firefox)* in the Art Window menu.

A painting goes in within a few seconds. It stays for the whole of the meeting
that shows it, and the moment that meeting ends the next one takes its place, so
a meeting started straight after another still gets a new painting. The
paintings follow the filters in the settings tab and are chosen to suit a 16:9
camera frame. The wallpaper and the day's picture are not affected, and the two
apps show different paintings.

Unticking a row stops the changes and leaves the last painting in the app. If
you remove or replace that background in the app, add a picture again and tick
the row again.

How it works, and what has not been verified:
[`meeting-backdrops.md`](meeting-backdrops.md).

## Settings

`config.toml` lives at the location `--where` reports. It is read and never
rewritten, so comments survive.

```toml
# "museums" for public-domain paintings from the Met, the National Gallery of
# Art, the Cleveland Museum of Art and SMK, "met" to search the Metropolitan
# Museum's collection live instead, or a path to a folder of your own pictures.
source = "museums"
```

`"museums"` is the default for new installs: a prebuilt, pixel-verified list of
paintings from all four museums, checked in at `catalogue/dist/paintings.tsv`
and compiled straight into the binary, so a day's painting is one download
rather than a live search. `"met"` keeps the older behaviour — a live search
against just the Met's own collection — for a `config.toml` that already
spells it out. Regenerate the list with:

```sh
python3 catalogue/build.py            # all four museums; the Met pass alone takes ~3 h, resumable
python3 catalogue/build.py --only nga,cma,smk  # skip the Met for a quick rebuild of the rest
python3 catalogue/build.py --only wmc  # the painters named in catalogue/artists.json
```

The build ends with the number of paintings per region and per artist, then
`art-window --catalogue`'s table of every region, subject and shape together,
with anything under twenty marked. A marked cell is a choice the settings tab
will grey; the cure is more painters in `catalogue/artists.json`.

One painting a day is the whole schedule and there is nothing to tune. A
`refresh_hours` left over from an older version is accepted and ignored so an
existing config keeps working.

Pointing `source` at a folder inside `~/Pictures` or `~/Documents` can make macOS
ask for access. A launchd process cannot show that prompt, so run
`art-window --once` from a terminal to approve it.

Settings are read when Art Window starts. After editing `config.toml`, quit and
reopen it.

The settings tab writes its choices to `settings.json`, beside `state.json`. It
is not meant to be edited by hand; deleting it restores the defaults.

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

On Windows, uninstall *Art Window* from *Settings → Apps*. That stops it and removes
the sign-in entry. Your settings, state and favourites are kept; delete them with:

```powershell
Remove-Item -Recurse "$env:APPDATA\ArtWindow", "$env:LOCALAPPDATA\ArtWindow"
```

Adjust those paths if the installer or XDG directories were overridden. The
wallpaper stays as it is; choose another in system settings to change it back.

## Releasing

Bump `version` in `Cargo.toml`, then run `cargo build` so `Cargo.lock` follows.
Commit both, then tag and push:

```sh
git tag vX.Y.Z
git push origin vX.Y.Z
```

The [release workflow](../.github/workflows/release.yml) checks that the tag
matches the `Cargo.toml` version, then builds and publishes the macOS DMG, the
Linux tarball, the Windows installer, the Android APK, and a `SHA256SUMS` file against the tag on
GitHub. The Android `versionCode` is derived as `major*10000 + minor*100 +
patch`, so every release must increase the version.

Release assets are published under version-less names — `art-window-macos.dmg`,
`art-window-linux-x86_64.tar.gz`, `art-window-windows-x64-setup.exe`,
`art-window-android.apk` — because the README
links straight to `releases/latest/download/<name>`. Renaming them breaks those
links without any error.

**One-time keystore setup**, for signing the Android release build:

```sh
keytool -genkeypair -v -keystore art-window.jks -keyalg RSA -keysize 4096 \
  -validity 10000 -alias art-window
base64 -i art-window.jks | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set ANDROID_KEYSTORE_PASSWORD
gh secret set ANDROID_KEY_ALIAS
gh secret set ANDROID_KEY_PASSWORD
```

Keep `art-window.jks` out of the repo (gitignored) and backed up somewhere
durable. Losing it is not recoverable: every user would have to uninstall the
app, losing their favourites, before the next release could install over it.

The DMG is ad-hoc signed, not notarized — notarizing would need a paid Apple
Developer ID.

## Credits

Artwork metadata and images come from [The Metropolitan Museum of Art
Collection API](https://metmuseum.github.io/), the [National Gallery of Art's
open data](https://github.com/NationalGalleryOfArt/opendata), the [Cleveland
Museum of Art's Open Access API](https://openaccess-api.clevelandart.org/),
and [SMK's API](https://api.smk.dk/) — all public domain or CC0.
