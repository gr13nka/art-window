//! One turn of the rotation: find the next picture, then put it up.
//!
//! Split in two because the halves want opposite things. `fetch` talks to a museum
//! over the network and can block for two minutes, so it belongs anywhere but the
//! thread drawing the menu bar; `show` reaches into AppKit and so may run *only* on
//! that thread. The tray carries an `Artwork` from one to the other; the one-shot
//! command, having no menu to freeze, simply calls both in a row.

use crate::art::{folder::Folder, met, met::Met, Artwork, Source};
use crate::config::{self, Config, Paths, State};
use crate::wallpaper;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Downloads the next picture, avoiding whatever is on the desktop now.
///
/// Slow by nature, and safe to call from a worker thread.
pub fn fetch(config: &Config, state: &State, cache: &Path) -> Result<Artwork> {
    let source = build_source(config, state);
    source
        .fetch(cache)
        .with_context(|| format!("fetching from {}", source.label()))
}

/// Puts `artwork` on the desktop and records it as the picture of the day.
///
/// The clock advances only once the wallpaper is actually up, so a failure here
/// leaves the day unspent and the next attempt retries.
///
/// Must run on the main thread — see [`wallpaper::pin`].
pub fn show(artwork: &Artwork, paths: &Paths, state: &mut State) -> Result<()> {
    wallpaper::pin(&artwork.path)?;

    state.last_success = Some(config::now_secs());
    state.shown = Some(artwork.clone());
    state.save(&paths.state)?;

    tidy_cache(&paths.cache, &artwork.path);
    Ok(())
}

fn build_source(config: &Config, state: &State) -> Box<dyn Source> {
    let shown = state.shown.as_ref();
    if config.source == "met" {
        Box::new(Met::new(shown.and_then(|a| met::id_of(&a.path))))
    } else {
        Box::new(Folder::new(
            expand_tilde(&config.source),
            shown.map(|a| a.path.clone()),
        ))
    }
}

/// Keeps only the picture now on screen. Deleting it while it is the wallpaper
/// would leave a blank desktop, so the current file is always spared.
fn tidy_cache(cache: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(s),
    }
}
