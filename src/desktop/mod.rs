//! The handful of operations Art Window asks of the desktop around it.
//!
//! Wallpaper placement, opening a web page and starting at login are expressed
//! differently by every desktop. Keeping them behind one seam leaves rotation and
//! presence concerned only with what the user asked for, not where it is running.

use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "linux")]
pub const APP_ID: &str = "dev.artwindow";
#[cfg(target_os = "linux")]
pub const QUIT_ACTION: &str = "quit";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

/// How much of the desktop a picture actually reached.
///
/// A desktop can be more than one surface — macOS keeps a wallpaper for every
/// Mission Control Space on every display — and the store holding the ones the
/// user is not looking at is not always there to be written. A picture that
/// reached only the surface in front of them is not a failure worth undoing the
/// day for; it is a reason to ask again shortly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pinned {
    /// The whole desktop is showing it, and nothing further is owed.
    Everywhere,
    /// Part of the desktop is showing it and the rest kept what it had, so this
    /// has to be asked for again.
    InPart,
}

/// Shows `path` on every display, scaled to fit entirely on screen with black
/// filling the margins, and holds that placement against the things that would
/// otherwise reset it.
///
/// Re-asserting the placement is deliberately not the caller's job. A caller that
/// had to remember it would eventually forget, which is exactly the bug this
/// program exists to stop happening. What a caller does own is [`Pinned::InPart`]:
/// the desktop was not in a state to take the picture whole, and only the caller
/// knows when it is worth interrupting to try again.
///
/// A backend may require this to run on the main thread. macOS does, because
/// AppKit will only enumerate displays there; the GNOME backend has no such
/// affinity.
pub fn pin(path: &Path) -> Result<Pinned> {
    let path = path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot read artwork at {}: {e}", path.display()))?;
    platform::pin(&path)
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

#[cfg(target_os = "linux")]
pub fn quit_running() -> Result<()> {
    platform::quit_running()
}
