//! The handful of operations Art Window asks of the desktop around it.
//!
//! Wallpaper placement, opening a web page and starting at login are expressed
//! differently by every desktop. Keeping them behind one seam leaves rotation and
//! presence concerned only with what the user asked for, not where it is running.

use crate::placement;
use crate::settings::{Framing, Style};
use anyhow::Result;
use std::path::Path;

/// The GApplication id, and one link in a chain that breaks silently if any of
/// it is changed alone: the `/dev/artwindow` object path `--quit` calls
/// (`linux::quit_running`), the autostart file name (`linux/login.rs`), and the
/// file name, `Icon` and `StartupWMClass` of `linux/dev.artwindow.desktop.in`.
#[cfg(target_os = "linux")]
pub const APP_ID: &str = "dev.artwindow";
#[cfg(target_os = "linux")]
pub const QUIT_ACTION: &str = "quit";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// How much of the desktop a picture actually reached.
///
/// A desktop can be more than one surface — macOS keeps a wallpaper for every
/// Mission Control Space on every display — and the surface in front of the user is
/// the only one that answers immediately. The rest are written to, and what becomes
/// of that writing is the difference between these three.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pinned {
    /// The backend had nothing new to publish: every surface updates immediately,
    /// or its store already named this picture. A caller may still retain an older
    /// redraw debt for that same path.
    Everywhere,
    /// It is up where the user is looking, and written for everywhere else — but
    /// those surfaces go on showing the old picture until the desktop is next
    /// redrawn. See [`catch_up`].
    AfterRedraw,
    /// It is up where the user is looking and nowhere else: the rest of the desktop
    /// would not take the writing at all, so this has to be asked for again.
    InPart,
}

/// Shows what [`Pinned::AfterRedraw`] left written but not yet visible.
///
/// Disruptive, and that is the whole reason it is a separate call: on macOS it
/// restarts the Dock, which blanks every desktop until the Dock is back — half a
/// minute on a tired machine. So it belongs to moments when the desktop is being
/// redrawn anyway, waking and beginning a session, rather than to the moment a
/// picture changes. What the user is actually looking at is never waiting on this;
/// that went up when it was pinned.
pub fn catch_up(reason: &str) {
    platform::catch_up(reason);
}

/// Shows `path` on every display placed as `style` says — fitted over coloured
/// margins, zoomed, stretched, or over a blur of itself — moved about as `framing`
/// says, and holds that placement against the things that would otherwise reset
/// it. A picture that has to be composed (see [`placement::resolve`]) is drawn
/// into `scratch`.
///
/// Re-asserting the placement is deliberately not the caller's job. A caller that
/// had to remember it would eventually forget, which is exactly the bug this
/// program exists to stop happening. What a caller does own is *when to interrupt
/// the user* — see [`Pinned::InPart`], which is worth asking again about, and
/// [`Pinned::AfterRedraw`], which is worth showing at a moment of the caller's
/// choosing. Neither is knowable from here.
///
/// A backend may require this to run on the main thread. macOS does, because
/// AppKit will only enumerate displays there; the GNOME backend has no such
/// affinity.
pub fn pin(path: &Path, style: &Style, framing: &Framing, scratch: &Path) -> Result<Pinned> {
    let path = path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot read artwork at {}: {e}", path.display()))?;
    let mut hang = placement::resolve(&path, style, framing, primary_screen(), scratch)?;
    hang.path = hang
        .path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", hang.path.display()))?;
    platform::pin(&hang)
}

/// The main display's size in device pixels, width first — what a composed
/// picture is drawn at, and what the shape filter measures paintings against.
///
/// Main-thread only on macOS, for the same reason as [`pin`]. Falls back to a
/// common laptop panel if the desktop will not say.
pub fn primary_screen() -> (u32, u32) {
    platform::primary_screen()
        .filter(|&(w, h)| w > 0 && h > 0)
        .unwrap_or((2560, 1600))
}

/// [`primary_screen`] as width ÷ height.
pub fn primary_aspect() -> f64 {
    let (w, h) = primary_screen();
    f64::from(w) / f64::from(h)
}

/// Opens `url` in the desktop's default browser.
pub fn browse(url: &str) {
    platform::browse(url);
}

/// Whether Art Window is registered to start at the next login.
pub fn starts_at_login() -> bool {
    platform::starts_at_login()
}

/// Registers or unregisters Art Window for the next login.
pub fn set_start_at_login(enabled: bool) -> Result<()> {
    platform::set_start_at_login(enabled)
}

/// Prints GNOME integration capabilities and verifies the current wallpaper when
/// one is recorded.
#[cfg(target_os = "linux")]
pub fn check(shown: Option<&Path>) -> Result<()> {
    platform::check(shown)
}

#[cfg(target_os = "linux")]
pub(crate) use platform::TrayHostWatch;

#[cfg(target_os = "linux")]
pub fn appindicator_available() -> bool {
    platform::appindicator_available()
}

#[cfg(target_os = "linux")]
pub fn watch_tray_host(on_changed: impl Fn(bool) + 'static) -> Result<TrayHostWatch> {
    platform::watch_tray_host(on_changed)
}

/// Asks the Art Window already running in this session to exit.
#[cfg(any(target_os = "linux", windows))]
pub fn quit_running() -> Result<()> {
    platform::quit_running()
}

#[cfg(windows)]
pub(crate) use platform::Instance;

/// Claims this session for one Art Window, or answers `None` when another already
/// holds it. GNOME does this through `GApplication`; Windows has nothing of the
/// kind, so the claim is made here. `on_quit` is called — on a system thread, hence
/// `Send` — when a later `--quit` asks the holder to go.
#[cfg(windows)]
pub fn claim_instance(on_quit: impl Fn() + Send + Sync + 'static) -> Result<Option<Instance>> {
    platform::claim_instance(on_quit)
}

/// Whether the taskbar is drawn light, which decides the ink of the tray glyph.
#[cfg(windows)]
pub fn light_taskbar() -> bool {
    platform::light_taskbar()
}

/// Lets a command-line mode print to the terminal it was started from. A release
/// build is a GUI-subsystem program, which Windows starts with no console at all.
#[cfg(windows)]
pub fn attach_console() {
    platform::attach_console()
}

/// Sends everything written to stderr to `path` from now on. Under the GUI
/// subsystem stderr goes nowhere, and this is where the whole error chain that the
/// menu shortens is kept — the counterpart of the launchd agent's log on macOS.
#[cfg(windows)]
pub fn log_to(path: &Path) {
    platform::log_to(path)
}
