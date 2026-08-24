# GNOME wallpaper integration

Art Window's Linux backend targets a GNOME session with GTK 3, GSettings, and a
session D-Bus. It deliberately does not pretend that writing a generic wallpaper
file is portable across Linux desktops.

## What a wallpaper write does

`desktop::pin` canonicalizes the artwork path and turns it into a `file://` URI.
The GNOME backend first checks that `org.gnome.desktop.background` is installed;
constructing `gio::Settings` for a missing schema can abort the process instead of
returning an error.

It then writes these keys as one operation:

- `picture-options = 'scaled'`, so the whole image is visible;
- `primary-color = '#000000'` and `color-shading-type = 'solid'`, for black
  letterboxing;
- `picture-uri`, plus `picture-uri-dark` when that key exists.

GSettings can discard a same-value write without notifying GNOME Shell. Re-apply
therefore clears the URI keys before restoring the real URI, calls
`gio::Settings::sync`, and reads `picture-uri` back. Placement belongs to `pin`, so
callers cannot accidentally change the picture without reasserting fit and black
margins.

The write is refused when `DBUS_SESSION_BUS_ADDRESS` is absent. A transient
GSettings backend can otherwise report success while nothing reaches the logged-in
desktop.

## Diagnosis

Run this from a terminal inside the affected GNOME login:

```sh
art-window --check
```

It reports the session bus, background schema, optional dark URI key, current URI,
StatusNotifier host, and AppIndicator runtime. If state records a shown artwork,
the command also re-applies it and verifies the readback. All hard failures are
collected before the command exits nonzero.

The underlying values can also be inspected directly:

```sh
gsettings get org.gnome.desktop.background picture-options
gsettings get org.gnome.desktop.background picture-uri
gsettings get org.gnome.desktop.background picture-uri-dark
```

The last command is expected to fail on GNOME versions whose schema has no dark
key; Art Window probes it before writing.

## Window and panel presence

The GTK window contains the daily controls and accessible favourites list. An
AppIndicator menu is added only when both an AppIndicator library and a running
`org.kde.StatusNotifierWatcher` are available. If either is missing, the window is
the complete fallback—no GNOME Shell extension is required.

Art Window watches the tray host's D-Bus owner, so installing, enabling, disabling,
or restarting an indicator extension changes surfaces without restarting the app.
Launching Art Window again brings the existing instance's window forward. Use
`art-window --quit` to stop that instance from a terminal.

## Session lifecycle

Start at login writes
`$XDG_CONFIG_HOME/autostart/dev.artwindow.desktop` (or
`~/.config/autostart/dev.artwindow.desktop`) with the current executable's absolute
path. It neither starts nor stops the current process.

The resident process listens for logind's `PrepareForSleep(false)` signal on the
system bus and immediately rechecks the local calendar day after wake. Failure to
subscribe is nonfatal because the GTK main-context timer still prompts a check at
least once a minute.
