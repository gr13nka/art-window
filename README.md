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
does not currently build for them. There is no menu-bar icon yet — Art Window is a
command, scheduled by launchd.

## Install

```sh
cargo build --release
cp target/release/art-window ~/.local/bin/
codesign --force --sign - ~/.local/bin/art-window
```

To have it run by itself, create `~/Library/LaunchAgents/dev.artwindow.daily.plist`
pointing at the binary with `--if-due`, `RunAtLoad` set, and `StartInterval` of
3600, then:

```sh
launchctl bootstrap gui/$UID ~/Library/LaunchAgents/dev.artwindow.daily.plist
```

launchd wakes it hourly; the program itself decides whether a new picture is owed,
so the check is a few milliseconds and logging in repeatedly does not burn through
paintings.

## Use

```sh
art-window            # fetch a new painting now
art-window --if-due   # fetch only if the current one has had its time
art-window --where    # print where settings, state and cache live
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
ask for access. A launchd-run process cannot show that prompt, so run `art-window`
once from a terminal to approve it.

## Uninstall

```sh
launchctl bootout gui/$UID/dev.artwindow.daily
rm ~/Library/LaunchAgents/dev.artwindow.daily.plist ~/.local/bin/art-window
rm -rf ~/Library/Application\ Support/ArtWindow
```

Your wallpaper stays as it is. To change it back, pick another in System
Preferences.

## Credits

Artwork metadata and images from [The Metropolitan Museum of Art Collection
API](https://metmuseum.github.io/), used under its open-access terms.
