//! Puts artwork on the desktop and keeps its placement from drifting.
//!
//! Every platform stores wallpaper placement separately from the wallpaper itself,
//! and each one loses that setting under different circumstances. Keeping the two
//! in agreement is this module's whole job; nothing outside it needs to know how
//! any particular desktop expresses "fit the screen, letterboxed in black".

use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

/// Shows `path` on every display, scaled to fit entirely on screen with black
/// filling the margins, and holds that placement against the things that would
/// otherwise reset it.
///
/// Re-asserting the placement is deliberately not the caller's job. macOS records
/// placement per image *path*, so every new artwork — a new path — arrives with the
/// system default rather than the placement chosen here. A caller that had to
/// remember this would eventually forget, which is exactly the bug this program
/// exists to stop happening.
///
/// # Platform notes
///
/// Must be called from the main thread: AppKit will only enumerate displays there.
/// This is enforced with an error rather than documented and hoped for.
pub fn pin(path: &Path) -> Result<()> {
    let path = path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot read artwork at {}: {e}", path.display()))?;
    platform::pin(&path)
}
