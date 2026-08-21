//! One turn of the rotation: find the next picture, then put it up.
//!
//! Split in two because the halves want opposite things. `fetch` talks to a museum
//! over the network and can block for two minutes, so it belongs anywhere but the
//! thread drawing the menu bar; `show` reaches into AppKit and so may run *only* on
//! that thread. The tray carries an `Artwork` from one to the other; the one-shot
//! command, having no menu to freeze, simply calls both in a row.

use crate::art::{self, Artwork};
use crate::config::{Config, Paths, State};
use crate::wallpaper;
use anyhow::{Context, Result};
use std::path::Path;

/// Downloads the next picture, avoiding whatever is on the desktop now.
///
/// Slow by nature, and safe to call from a worker thread.
pub fn fetch(config: &Config, state: &State, cache: &Path) -> Result<Artwork> {
    let source = art::source_for(&config.source, cache);
    source
        .fetch(state.shown.as_ref())
        .with_context(|| format!("fetching from {}", source.label()))
}

/// Puts `artwork` on the desktop and records it as the picture of the day.
///
/// The clock advances only once the wallpaper is actually up, so a failure here
/// leaves the day unspent and the next attempt retries.
///
/// Must run on the main thread — see [`wallpaper::pin`].
pub fn show(artwork: &Artwork, config: &Config, paths: &Paths, state: &mut State) -> Result<()> {
    wallpaper::pin(&artwork.path)?;
    state.record_shown(artwork, &paths.state)?;

    // Built again rather than carried over from `fetch`, which ran on another
    // thread. Clearing up is the source's own business either way: it is the only
    // thing that knows which files in the cache are its doing.
    art::source_for(&config.source, &paths.cache).discard_all_but(&artwork.path);
    Ok(())
}
