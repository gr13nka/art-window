//! Where a day's artwork comes from.
//!
//! A source's job ends with a file on disk and enough about it to name in a menu.
//! Callers get no say in — and no sight of — how that happened: which host, how many
//! requests, what the JSON looked like.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod folder;
pub mod met;

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

pub trait Source {
    /// Finds a picture and puts it in `dir`, returning it with its metadata.
    ///
    /// Implementations own their own retries: a source that has to sift candidates
    /// to find a usable one does so here rather than making the caller loop.
    fn fetch(&self, dir: &Path) -> Result<Artwork>;

    /// Name for this source in the menu.
    fn label(&self) -> &'static str;
}

/// A random `u64` without a `rand` dependency.
///
/// `RandomState` is seeded by the OS once per process; hashing a counter off it
/// yields values that differ between runs, which is all that picking a daily
/// painting requires. Nothing here is security-sensitive.
pub(crate) fn random_u64(counter: u64) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(counter);
    h.finish()
}
