//! The handful of operations Art Window asks of the desktop around it.
//!
//! Wallpaper placement, opening a web page and starting at login are expressed
//! differently by every desktop. Keeping them behind one seam leaves rotation and
//! presence concerned only with what the user asked for, not where it is running.

use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

/// Shows `path` on every display, scaled to fit entirely on screen with black
/// filling the margins, and holds that placement against the things that would
/// otherwise reset it.
///
/// Re-asserting the placement is deliberately not the caller's job. A caller that
/// had to remember it would eventually forget, which is exactly the bug this
/// program exists to stop happening.
///
/// A backend may require this to run on the main thread. macOS does, because
/// AppKit will only enumerate displays there; the GNOME backend has no such
/// affinity.
pub fn pin(path: &Path) -> Result<()> {
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
