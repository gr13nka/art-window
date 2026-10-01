# Art Window for iPhone and iPad

The iOS app is a Swift port of the Android one (see [android.md](android.md)): the
same catalogue, the same filters, the same rendering styles. It differs in one
place, and that place decides the whole shape of the app.

## iOS will not let an app set the wallpaper

There is no public API for it. `WallpaperManager` has no iOS equivalent, and the
private ones are not an option for anything that has to keep working. The one
sanctioned route is the Shortcuts app's **Set Wallpaper** action, which accepts an
image from any earlier step. So the app does not set the wallpaper. It hands
over a picture ready to be set:

- **Get Today's Painting** (`GetTodaysPaintingIntent`) settles the day if a
  painting is owed, then returns the shown painting as a PNG rendered at exactly
  the wallpaper's pixel size in the saved style. Placement is baked into the pixels,
  as on Android, so *Set Wallpaper* has nothing left to crop.
- **Next Picture** (`NextPictureIntent`) does the same after forcing a new
  download, and so spends the day, like the desktop's `record_fetched`.
- A **personal automation** joins them: Time of Day, daily at 05:05, *Run
  Immediately*, Get Today's Painting → Set Wallpaper with *Show Preview* off. An app
  cannot create an automation for the user, so the app walks through the steps once
  on first launch and keeps the guide in Settings.

**The intent never fails because of the network.** An automation that fails at
05:05 leaves an error banner for the morning. So a download that fails or runs
past the intent's budget (about 25 s, under the system's limit) falls through
and returns the painting already shown. The failure still cools off, and
`isDue` still says the day is owed, so the next run retries. The only error
the intent throws is *nothing has ever been downloaded*.

**The download usually happens before the automation.** A `BGAppRefreshTask`
(`dev.artwindow.refresh`) is scheduled a few minutes after the next day begins —
05:00 local, `Day.beginsAtHour`, the desktop's rule — and calls `Rotation.turn(force: false)`. iOS runs these tasks when it
chooses, so this is only a head start. The intent is what guarantees a painting
is delivered. Opening the app does the same.

## Widgets observe, never fetch

`PaintingWidget` (small, medium and large; extra-large on iPad; the small size
also serves StandBy) draws `widget.jpg` from the App Group container.
`Rotation` writes that copy, downsampled to 1000 px on the long side, whenever
the shown painting changes, because a widget extension's memory limit would
not survive decoding an original of up to 30 MB. Rotation then reloads the
timelines. The timeline itself holds one entry that expires when the next day
begins. The widget has the same relationship to the painting as `ArtDream`
on Android TV: it shows what is there and asks for nothing.

Lock-screen accessory widgets are left out on purpose. The system renders them
monochrome or tinted, and a painting doesn't survive that.

## One container, three processes

The app, the intents (which run in the app's process) and the widget extension
share the App Group `group.dev.artwindow`, accessed through `SharedContainer`:

- `cache/`: the source's downloads, named `{source}-{id}.{ext}` as on Android;
- `favourites/`: copies, keeping the original name;
- `state.json`;
- `widget.jpg`;
- the preferences suite.

App Groups need a paid Apple Developer team; a free personal team cannot sign
this app.

## iPad: a square canvas

An iPad wallpaper serves both orientations, and iOS keeps the centre of it in
each. So on iPad `DeviceScreen.current` is a square the size of the long side,
and *Screen-shaped* means near-square. This needed no special case: `Screen`'s
maths never assumed an orientation, which is how the Android TV case came for
free as well.

`DeviceScreen` remembers the last canvas in the shared defaults. Intents and
the background task don't run on the main thread and have no scene to ask, so
they read the remembered value rather than `UIScreen`.

## Rules carried over

Each Swift file in `ios/ArtWindowKit/` names the Kotlin file it ports. The
invariants are the desktop's and Android's:

- `RotationState.isDue` compares local calendar days (`Day.local`). It is the only
  thing that decides a painting is owed; nothing counts down.
- Every failure cools off, and a failed download never consumes the day.
- `recordFetched` always stamps the day. `recordChosen` stamps it only when a
  painting was already owed, so choosing a favourite takes the screen without
  taking the day.
- Whoever wrote a file may delete it, but never the one being shown. The
  source's sweep spares `fetched`; the favourites' sweep spares `shown`.
- Subject matching, including the Danish words, and the portrait and religious
  exclusions now live **three** times: `Catalogue.kt`, `src/art/museums.rs`
  and `Catalogue.swift`. A word-list change belongs in all three.
- `Cargo.toml`'s version is the only version. `ios/remote.sh` reads it and
  passes `MARKETING_VERSION` and `CURRENT_PROJECT_VERSION`
  (`major*10000 + minor*100 + patch`, the same number as the Android
  `versionCode`).

## Building

This development machine's Xcode 14.2 can't build for iOS 17, so builds run on
`ios_macmini` (Xcode 26), the same way Android builds do. The project file is
generated from `ios/project.yml` by XcodeGen, which is unpacked on the mini at
`~/opt/xcodegen` rather than installed through Homebrew. The `.xcodeproj` is
not checked in.

```sh
./ios/remote.sh           # generate, build and run ArtWindowKit's unit tests on a simulator
./ios/remote.sh build     # signed device build (DEVELOPMENT_TEAM set on the mini)
./ios/remote.sh install   # build and install on a device paired with the mini
```

The catalogue is bundled from `catalogue/dist/paintings.tsv` in place, so no copy
lives under `ios/`.

## Verifying on a device

The unit tests cover the ported rules. They can't show that a wallpaper was
set. On the phone:

1. Build the automation from the in-app guide and run it by hand from
   Shortcuts. The wallpaper changes and the widget follows.
2. Turn on airplane mode and run it again. It still sets the current painting,
   with no error.
3. The next morning, the wallpaper is a new painting.

## Out of scope for now

- App Store and TestFlight release;
- Apple TV;
- lock-screen accessory widgets;
- the folder source.
