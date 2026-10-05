//! One turn of the rotation: find the next picture, then put it up — and the other
//! way a picture reaches the desktop, which is that somebody asked for it.
//!
//! `fetch` and `show` are split because the halves want opposite things. `fetch`
//! talks to a museum over the network and blocks for as long as its budget allows,
//! so it belongs anywhere but the thread drawing the menu bar; `show` reaches into
//! AppKit and so may run *only* on that thread. The tray carries an `Artwork` from one to the
//! other; the one-shot command, having no menu to freeze, simply calls both in a
//! row.
//!
//! `revisit` is `show` for a picture that was not fetched: a favourite, or the
//! day's own picture put back after one. The difference is entirely in what gets
//! recorded — the desktop cannot tell them apart, and neither can the wallpaper.

use crate::art::{self, Artwork, Selection};
use crate::config::{Config, Paths, State};
use crate::desktop;
use crate::journal;
use crate::settings::Settings;
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;

/// Downloads the next picture, avoiding whatever is on the desktop now.
///
/// Slow by nature, and safe to call from a worker thread — which is why
/// `selection` arrives already measured: the screen can only be asked about on
/// the main thread.
pub fn fetch(
    config: &Config,
    state: &State,
    cache: &Path,
    selection: &Selection,
) -> Result<Artwork> {
    let source = art::source_for(&config.source, cache, selection);
    journal::note!(
        "fetch",
        "start, source {:?}, filters {:?}, previous {}",
        config.source,
        selection.filters,
        state
            .shown
            .as_ref()
            .map_or("none".to_string(), |art| art.path.display().to_string())
    );
    let began = Instant::now();
    let fetched = source
        .fetch(state.shown.as_ref())
        .with_context(|| format!("fetching from {}", source.label()));
    let ms = began.elapsed().as_millis();
    match &fetched {
        Ok(art) => journal::note!(
            "fetch",
            "done in {ms} ms, title {:?}, path {}",
            art.title,
            art.path.display()
        ),
        Err(error) => {
            journal::note!("fetch", "failed after {ms} ms");
            journal::fault("fetch", error);
        }
    }
    fetched
}

/// Puts the picture the rotation has just found on the desktop, as the day's.
///
/// The clock advances only once the wallpaper is actually up, so a failure here
/// leaves the day unspent and the next attempt retries. A picture that reached
/// only part of the desktop *has* arrived and does spend the day — it is the
/// desktop that is not ready, not the museum — so the answer is handed back for
/// the caller to ask again with, rather than turned into a fresh download.
///
/// Kept on the event-loop thread because the macOS [`desktop::pin`] backend
/// requires it.
pub fn show(
    artwork: &Artwork,
    settings: &Settings,
    config: &Config,
    paths: &Paths,
    state: &mut State,
) -> Result<desktop::Pinned> {
    let pinned = hang(artwork, settings, paths, "show")?;
    state.record_fetched(artwork, &paths.state)?;
    sweep(config, paths, state);
    Ok(pinned)
}

/// Puts a picture somebody has picked on the desktop — a favourite, or the day's
/// own picture put back after one.
///
/// The day's picture is unchanged by this, which is the whole point: it is what
/// there is to come back to. Neither, in the ordinary case, is the schedule — see
/// [`State::record_chosen`].
///
/// Kept on the event-loop thread because the macOS [`desktop::pin`] backend
/// requires it.
pub fn revisit(
    artwork: &Artwork,
    settings: &Settings,
    config: &Config,
    paths: &Paths,
    state: &mut State,
) -> Result<desktop::Pinned> {
    let pinned = hang(artwork, settings, paths, "revisit")?;
    state.record_chosen(artwork, &paths.state)?;
    sweep(config, paths, state);
    Ok(pinned)
}

/// Hangs `artwork` the way the settings say: their style and their framing, and
/// nothing of their filters, which chose the picture and have no say in placing it.
fn hang(
    artwork: &Artwork,
    settings: &Settings,
    paths: &Paths,
    why: &str,
) -> Result<desktop::Pinned> {
    journal::note!(
        "rotation",
        "{why} {}, style {:?}, framing {:?}",
        artwork.path.display(),
        settings.style,
        settings.framing
    );
    let pinned = desktop::pin(
        &artwork.path,
        &settings.style,
        &settings.framing,
        &paths.cache,
    );
    match &pinned {
        Ok(answer) => journal::note!("rotation", "{why} hung, {answer:?}"),
        Err(error) => journal::fault("rotation", error),
    }
    pinned
}

/// Lets the source clear up after itself, sparing the day's picture.
///
/// The day's file, and deliberately not the desktop's. A favourite on the desktop
/// belongs to the favourites folder and is nothing to do with the source; today's
/// download, meanwhile, has to outlive being switched away from or there would be
/// nothing to come back to. Getting these the wrong way round is what made coming
/// back impossible in the first place.
///
/// The source is built again rather than carried over from `fetch`, which ran on
/// another thread. Clearing up is its own business either way: it is the only thing
/// that knows which files in the cache are its doing.
fn sweep(config: &Config, paths: &Paths, state: &State) {
    let todays = state.fetched.as_ref().map(|art| art.path.as_path());
    journal::note!(
        "rotation",
        "sweep, sparing {}",
        todays.map_or("nothing".to_string(), |path| path.display().to_string())
    );
    art::source_for(&config.source, &paths.cache, &Selection::default()).discard_all_but(todays);
}
