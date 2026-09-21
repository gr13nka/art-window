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

Art Window supports macOS and Linux with GNOME as desktop wallpaper apps, and
Android as a native Kotlin phone app. The GNOME port uses GTK 3, GSettings,
logind, and the XDG directory conventions. Windows is not implemented.

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
painting changes on the first hourly check after midnight, over Wi-Fi.

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

## Releasing

Bump `version` in `Cargo.toml`, then run `cargo build` so `Cargo.lock` follows.
Commit both, then tag and push:

```sh
git tag vX.Y.Z
git push origin vX.Y.Z
```

The [release workflow](../.github/workflows/release.yml) checks that the tag
matches the `Cargo.toml` version, then builds and publishes the macOS DMG, the
Linux tarball, the Android APK, and a `SHA256SUMS` file against the tag on
GitHub. The Android `versionCode` is derived as `major*10000 + minor*100 +
patch`, so every release must increase the version.

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

Artwork metadata and images come from [The Metropolitan Museum of Art Collection
API](https://metmuseum.github.io/), under its open-access terms.
