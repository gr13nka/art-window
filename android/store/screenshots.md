# Screenshots and graphics

Take them from a debug build (`./android/install.sh`). Play wants PNG or JPEG, 16:9 or 9:16, each side 320 to 3840 px. Nothing here is created yet.

## Phone (at least 2, up to 8)

1. Home screen with a landscape painting as the wallpaper, no app open.
2. Lock screen with the same painting.
3. The app's main screen: today's painting, its caption and the "See it at ..." link.
4. Settings, Shape and Origins sections unfolded.
5. Settings, Subjects and Artists sections, plus the preview of the placement style.
6. Favourites list with several paintings.

## Android TV (needed for the TV form factor; Play reviews it separately)

Take them from an emulator or a device at 1920 x 1080.

1. Art mode (`TvActivity`): a painting filling the screen.
2. Art mode with the OK-button overlay open (Next picture, Favourite, Favourites, Settings, Use as screensaver).
3. The screensaver (`ArtDream`) showing a painting with its caption.
4. Settings on TV with the D-pad focus ring visible.

## Graphics

- TV banner, 640 x 360: `android/app/src/main/res/drawable-xhdpi/tv_banner.png` (the manifest's `android:banner`). Play asks for 1280 x 720 as the TV banner upload; this file is half that size, so re-export it from `icon/icon.svg` via `icon/make.sh` or scale it up and check it is not soft.
- App icon, 512 x 512 PNG: not checked in at that size. Export it from `icon/icon.svg`.
- Feature graphic, 1024 x 500: not in the repo. Needs making.
