# Windows wallpaper integration

`src/desktop/windows/` puts a painting up through the shell's `IDesktopWallpaper`
COM object and holds the placement the same way the other backends do: every
`pin` sets the black background colour, the *Fit* position and the picture
together, so nothing else has to remember to.

## What `pin` does

1. `CoInitializeEx` apartment-threaded on the calling thread — the event loop's,
   as on macOS. A thread already initialised the other way is tolerated.
2. `SetBackgroundColor(0)`: the letterbox is black.
3. `SetPosition(DWPOS_FIT)`: the whole painting, never cropped.
4. `SetWallpaper(NULL, path)`: a null monitor id means every monitor.
5. Reads `GetWallpaper` back for each monitor and fails if any disagrees, the
   same read-back rule as the GNOME backend. So it answers `Pinned::Everywhere`
   or an error, and `catch_up` has nothing to publish.

## The verbatim-path trap

`desktop::pin` canonicalises the path before handing it to a backend. On Windows
`canonicalize` returns the verbatim form, `\\?\C:\Users\…` (or
`\\?\UNC\server\share\…`), and the shell's wallpaper and thumbnail APIs reject it.
The Windows backend and the favourites window each strip that prefix before they
call the shell. The fix stays inside the Windows code so the shared seam stays as
it is.

## Not yet verified

This backend has been built and checked by CI only; nobody has watched it on a
real desktop yet. In particular:

- **Virtual desktops on Windows 11.** Windows 11 can keep a different wallpaper for
  each virtual desktop. Whether `IDesktopWallpaper::SetWallpaper` reaches all of
  them or only the one in front is unchecked. If it turns out to be only the
  current one, that is Windows' version of the macOS Spaces trap (see
  [macos-wallpaper.md](macos-wallpaper.md)) and belongs in `Pinned::InPart` or
  `AfterRedraw`, not in callers.
- The tray glyph's ink comes from `SystemUsesLightTheme`, read once at start.
- The installer's autostart task and the tray's **Start at login** row both
  write the same `HKCU\…\Run` value, `ArtWindow`. They must keep the same name,
  or the row would tell a lie about what happens at sign-in.

To check a real machine, run `art-window --once` from a terminal. Then look at
*Settings → Personalization → Background*: it should show the picture, with *Fit*
and a black colour selected. Then switch virtual desktops.
