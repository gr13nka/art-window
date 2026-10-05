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

## A filter set must leave twenty paintings

`Catalogue.minPool` (20) mirrors `MIN_POOL` in `src/art/museums.rs` and `Catalogue.kt`,
on purpose. A pool of a handful is the same few pictures coming round again, so:

- a chip in Settings is offered only when at least twenty paintings pass with it
  chosen alone and the other sections held as staged (one already selected stays
  visible so it can be removed);
- *Apply changes* is disabled while the staged filters leave fewer than twenty, with
  "Only N paintings match these filters — at least 20 are needed", or "No painting
  matches these filters — set one section to Any" for none. Filters already applied
  are not held against a change of placement style, so a selection saved before the
  floor existed does not lock the tab;
- at fetch, `Catalogue.widened` relaxes the smallest set of active sections that
  restores twenty (single sections first, then pairs; ties in the order Shape,
  Origin, Subject, Artist, Content). Thin saved settings, a catalogue that shrank
  or *Phone-shaped* on another device therefore never alternate between a handful.
  If nothing cures it the filters are used as given and `nothingMatches` still fires.

## Choosing a painter by a painting

Fourteen names in one wrapping run of chips said nothing to someone who had not heard
them, so the Settings *Artist* section is two rows, exactly one on: *Any artist* (tapping
it clears the choice) and a row labelled with the chosen painter, or *Choose…*. One
painter at a time — choosing replaces whoever was chosen, and `Filters` decodes a
stored set of several (from an earlier build) as one name only. The second row opens a browser
modelled on `FavouritesView`: a grid of one picture per painter, ticked when chosen,
and a sheet for the one tapped with their painting large, a line of the form
"Golden summer, Eaglemont, 1889 · Oceania, 40 paintings", *Choose* — or *Chosen* on the one who is, there is no *Remove* — and
*Read more*, a `Link` that hands the Wikipedia article to Safari. On an iPad the grid
simply has more columns. Choosing only stages the name in the same `filters` Settings
holds; *Apply changes* still commits it. Painters run by region, in the `Region`
order, then by name. The count is `Catalogue.paintings(by:)`, portraits left out.

A chosen painter wins over Shape and Origins: choosing one means "show me their
work", so `Catalogue.admits` does not ask for region or shape while any artist is
staged, and Settings shows those two sections idle ("Not used while an artist is
chosen.") rather than hiding them. Subject and *Hide religious scenes* still narrow,
and so does the enlargement check. The twenty-painting floor goes too:
`Catalogue.needed(for:)` is 1 with a painter and `minPool` without, and `hasEnough`,
`widened` and the Apply rule all ask it, so the fetch never widens a painter away
while they have something to show. Relaxing the Artists section in `widened` brings
twenty back, because the threshold belongs to the filters being tested. Before this
a phone set to phone-shaped paintings from Europe could choose nobody.

A painter whose *Choose* is disabled is one whom Subject or *Hide religious scenes*
leaves with nothing (`Catalogue.whatEmpties`): "None of … paintings match the chosen
subject." or "… are left with religious scenes hidden." When only the enlargement
check empties them the first wording is used, which is slightly off.

The pictures are `catalogue/dist/artists/` (an `index.tsv` and one JPEG per painter),
referenced in place as a folder resource of `ArtWindowKit` like `paintings.tsv`, so
they land under `artists/` in the framework with no copy to drift. `Artists` parses
the index; a malformed row is skipped, and a painter the catalogue lists but the
index does not is left out of the browser. Thumbnails are downsampled by ImageIO as
the favourites ones are, and only the painter being looked at is decoded large.
Nothing is fetched, and neither the widget nor the intents read any of it. The rules
mirror `Pending::artist_row` and `artist_cards` on the desktop and the Android copy.

## Turning wide paintings (iPhone)

*Turn wide paintings*, off by default and absent on iPad, turns a painting wider than
tall 90° clockwise (its top ends at the screen's right edge) before it is hung on a
screen taller than wide. It is not a fifth style: Zoom, Stretch, Blur and Borders then
work on the turned picture. `RenderStyle.hung(width:height:on:)` is the one answer to
"the size as it will be hung"; the shape filter, `canRender` and `Renderer.render` all
ask it, so the pool is judged as the paintings will hang. The size check against the
catalogue's declared size in `Museums.fetch` still uses the file's own size. The
renderer gates on a portrait screen itself, so the square iPad canvas can never turn a
picture even with a stale preference. ImageIO's thumbnailer cannot rotate, so the
decoded thumbnail is drawn into a context of the swapped size; only `render` and
`edgeColours` do this. `thumbnail` stays upright, so the widget, the in-app picture
and favourites never turn. A saved style from before the option has no such key and
decodes with it off. *Apply changes* does not re-hang anything on iOS: the turned
wallpaper arrives with the next Shortcuts run.

## Framing the painting (iPhone)

Cover-cropped, a wide painting on a tall phone shows its centre strip, and the centre
is often not the part worth seeing. In Zoom, Blur (with a backdrop) and Borders the
Settings preview can be pinched and dragged to frame the sharp painting: `frameZoom`
runs from 1, the style's own size (cover for Zoom, fit for the other two), up to 3,
and the painting is cropped by the screen wherever it overflows, backdrop or border
showing only where it does not reach. Stretch and Blur's whole fill have no sharp
picture to frame, and the iPad's square canvas is not framed at all. A painting turned
by *Turn wide paintings* is framed like any other.

Zoom is a style option and outlives the painting. The position, `panX` and `panY`
(0 to 1 of the overflow on that axis, 0.5 centred), belongs to one painting:
`RenderStyle` keeps it with `panFor`, the file name it was set for, and
`effectivePan(for:)` is the only reader, answering the centre for any other painting,
so the next painting starts centred with nothing resetting it. A style saved with the
first version's single `pan` reads it into both axes. `canRender` and the shape filter
still judge a painting at zoom 1, so which paintings are admitted never depends on the
zoom.

`Framing.rect` in `Screen.swift` is the one geometry: where the painting's rectangle
sits relative to the screen. `Renderer` asks it at the screen's size and the preview at
its own, so they cannot disagree. The preview is two layers: what does not depend on
framing (the blurred backdrop, the border colour) is rendered once by `Renderer`, and
over it the sharp painting is an image of its own, decoded once, which SwiftUI offsets
and scales. A gesture changes three numbers and nothing is decoded until it ends,
when they are written to the staged style; the picture moves exactly as far as the
finger. The gestures are UIKit's, because only a UIKit pan can decline to begin: a drag
along an axis the picture cannot move on is refused at the start, so the Settings list
scrolls from a touch on the preview. Rendering a zoomed painting never decodes it larger
than its own pixels or 4096 px on the long side. The pan reaches the wallpaper with the
next Shortcuts run, which renders the same painting with the stored framing.

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
