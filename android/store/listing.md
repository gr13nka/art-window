# Play Console listing text

Paste each block as it is. Counts are characters including spaces and line breaks.

## Title (10 / 30)

```
Art Window
```

## Short description (57 / 80)

```
A new public-domain painting on your wallpaper every day.
```

## Full description (1520 / 4000)

```
Art Window puts a different painting on your phone every day.

Each morning the app sets a new public-domain painting as your wallpaper. It does this by itself. There is nothing to search and nothing to pick.

The app chooses paintings shaped like your screen, so the picture fills it. The default trims a painting lightly to do that. In settings you can allow squarer or wider paintings and choose how they sit on the screen.

About 18,000 paintings, drawn from six museums:
- The Metropolitan Museum of Art
- National Gallery of Art, Washington
- Cleveland Museum of Art
- SMK, the National Gallery of Denmark
- Rijksmuseum, Amsterdam
- J. Paul Getty Museum

All of them are in the public domain. You can narrow the choice by region, subject and artist.

Keep what you like. Mark a painting as a favourite and come back to it later. Every painting links to its page on the museum's own website, where you can read more.

On Android TV and Google TV, Art Window is also a screensaver and a full-screen art mode. The painting fills the screen, with a caption that shows for a few seconds.

How it works
- The app checks once an hour whether a new day has begun. A day starts at 5 am.
- A new painting downloads on Wi-Fi or another unmetered network.
- The painting is set as both your home screen and lock screen wallpaper.
- Next picture gives you another painting at any time.

Privacy
- No account.
- No ads.
- No analytics.
- The app sends nothing about you. It only downloads paintings from the museums.

The daily painting costs nothing.
```

## Facts to re-check before pasting

- "About 18,000": `catalogue/dist/paintings.tsv` held 18,012 rows on 2026-10-04.
- "Trims lightly" matches the default Phone-shaped setting in `docs/android.md`.
- "Once an hour" is `WATCH_INTERVAL_MS` and "unmetered" is `NETWORK_TYPE_UNMETERED` in `RotationJob.kt`; "5 am" is `Day.today()`.
- Home and lock screen: `Wallpaper.kt` writes `FLAG_SYSTEM or FLAG_LOCK`.
