//! Where a day's artwork comes from.
//!
//! A source's job ends with a file on disk and enough about it to name in a menu.
//! Callers get no say in — and no sight of — how that happened: which host, how many
//! requests, what the JSON looked like, where the file was put, or how the source
//! recognises the picture it put up last time.

use self::folder::Folder;
use self::met::Met;
use self::museums::Museums;
use crate::settings::Filters;
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};

pub mod artists;
pub mod folder;
pub(crate) mod http;
pub mod met;
pub mod museums;

/// One picture, ready to hang, with what a person would want to know about it.
///
/// Serialisable because the menu has to name the picture already on the desktop,
/// and after a restart the only witness to what that was is `state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artwork {
    /// "Wheat Field with Cypresses"
    pub title: String,
    /// "Vincent van Gogh, 1889" — artist and date, already joined for display.
    pub byline: String,
    /// Who to thank: "The Metropolitan Museum of Art".
    pub attribution: String,
    /// Where a curious viewer can read more. Absent for local files.
    pub details_url: Option<String>,
    /// The downloaded image.
    pub path: PathBuf,
}

/// Which collection a day's picture is drawn from.
///
/// How that choice is *spelled* in `config.toml` — the bare words `met` or
/// `museums`, or else a directory path, which a person may well have written
/// starting with `~/` — is this module's business and nobody else's. `Config`
/// hands the string over while reading the file and gets a decided value back, so
/// adding a source never reaches past here.
#[derive(Debug, Clone)]
pub enum SourceSpec {
    /// The Met's own API, searched live. Kept for configs already spelling it out;
    /// see [`SourceSpec::Museums`] for what new installs get instead.
    Met,
    /// The prebuilt, four-museum list compiled into the binary — no live search.
    Museums,
    Folder(PathBuf),
}

impl SourceSpec {
    pub fn parse(s: &str) -> Self {
        match s {
            "met" => Self::Met,
            "museums" => Self::Museums,
            path => Self::Folder(expand_tilde(path)),
        }
    }
}

impl<'de> Deserialize<'de> for SourceSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// `~/Pictures` means, in a settings file, what the person typing it meant by it.
fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(s),
    }
}

/// What the settings window has narrowed the choice to, and the shape of the
/// screen it is being narrowed for.
///
/// Handed to every source and honoured only by the ones that can: the museum
/// catalogue knows each painting's region, subject, artist and pixel size, while
/// the Met's live search and a folder of the user's own pictures know none of
/// that. Which source listens is the source's business, so nothing outside this
/// module has to ask.
#[derive(Debug, Clone)]
pub struct Selection {
    pub filters: Filters,
    /// The main display's width ÷ height.
    pub screen_aspect: f64,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            filters: Filters::default(),
            screen_aspect: 16.0 / 10.0,
        }
    }
}

/// The source `spec` asks for, set up to work in `cache` and choose within
/// `selection`.
///
/// Whether a source has any use for `cache` is its own affair — one downloads into
/// it, the other leaves the user's library where it lies — which is why the caller
/// hands it over once, here, and never again. The same goes for `selection`.
pub fn source_for(spec: &SourceSpec, cache: &Path, selection: &Selection) -> Box<dyn Source> {
    match spec {
        SourceSpec::Met => Box::new(Met::new(cache.to_path_buf())),
        SourceSpec::Museums => Box::new(Museums::new(cache.to_path_buf(), selection.clone())),
        SourceSpec::Folder(dir) => Box::new(Folder::new(dir.clone())),
    }
}

pub trait Source {
    /// Finds a picture and returns it with its metadata, passing over `avoid` if it
    /// can.
    ///
    /// `avoid` is the whole of the last picture shown rather than an identifier,
    /// because only a source knows how it recognises its own work — an object id
    /// spelled into a filename, a path, something else again — and that knowledge
    /// stays inside it.
    ///
    /// Implementations own their own retries: a source that has to sift candidates
    /// to find a usable one does so here rather than making the caller loop.
    fn fetch(&self, avoid: Option<&Artwork>) -> Result<Artwork>;

    /// Name for this source when a failure has to be reported.
    fn label(&self) -> &'static str;

    /// Deletes anything this source has left lying about, except `keep`.
    ///
    /// Called once the wallpaper is up, so `keep` is the file on the desktop and
    /// removing it would leave a blank one. Only whoever wrote a file may decide it
    /// is rubbish; a source that writes nothing implements this as nothing.
    fn discard_all_but(&self, keep: &Path);
}

/// Picks one of `len` items, which must not be none.
///
/// `nth` distinguishes repeated picks within a single run — the same `nth` gives the
/// same answer only by accident, which is all that choosing a painting requires.
pub(crate) fn pick_index(len: usize, nth: u64) -> usize {
    random_u64(nth) as usize % len
}

/// A random `u64` without a `rand` dependency.
///
/// `RandomState` is seeded by the OS once per process; hashing a counter off it
/// yields values that differ between runs, which is all that picking a daily
/// painting requires. Nothing here is security-sensitive.
fn random_u64(counter: u64) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(counter);
    h.finish()
}
