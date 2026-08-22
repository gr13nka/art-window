//! Pictures kept back from the rotation.
//!
//! The cache holds exactly one picture: whatever is on the desktop. Every rotation
//! deletes the rest, so liking a painting is not enough to see it again — the Met
//! has 490,000 objects and offers no second chances. Keeping one therefore means
//! taking a copy, into a folder this module owns outright, where nothing else is
//! entitled to tidy it away.
//!
//! What a copy is called, where the list is written, and when a copy may be deleted
//! are all this module's business. Callers keep pictures, drop them, and ask which
//! one a menu row meant.

use crate::art::Artwork;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The list, and the folder of copies it describes.
pub struct Favourites {
    dir: PathBuf,
    kept: Vec<Kept>,
}

/// One kept picture: the copy, and the original it was taken from.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Kept {
    /// Where the picture stood when it was kept — a download in the cache, or a
    /// file of the user's own. Never opened again: it is only how a picture on the
    /// desktop is recognised as one already kept, the copy having a path of its
    /// own that the sources know nothing about.
    origin: PathBuf,
    /// The picture as it now is, its `path` pointing at the copy.
    art: Artwork,
}

/// The list, written beside the copies it names so that the whole of a person's
/// favourites is one folder to find, copy or throw away.
const INDEX: &str = "index.json";

impl Favourites {
    /// Reads the list, treating an absent one as empty.
    ///
    /// Unlike `State`, a damaged file here is an error and not a fresh start. State
    /// is a convenience — losing it costs a repeated painting — whereas an empty
    /// list would be written straight back over the real one at the next `keep`.
    pub fn open(dir: &Path) -> Result<Self> {
        let index = dir.join(INDEX);
        let kept = match std::fs::read_to_string(&index) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("reading {}", index.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", index.display())),
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            kept,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    /// Whether `art` is already kept — whether it *is* one of the copies, or is the
    /// original one was taken from.
    pub fn holds(&self, art: &Artwork) -> bool {
        self.kept
            .iter()
            .any(|k| k.origin == art.path || k.art.path == art.path)
    }

    /// The picture a menu row meant, or nothing if it has since been dropped.
    pub fn get(&self, key: &str) -> Option<&Artwork> {
        self.kept.iter().find(|k| k.key() == key).map(|k| &k.art)
    }

    /// Every kept picture, each with the key that names it in a menu.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Artwork)> + '_ {
        self.kept.iter().map(|k| (k.key(), &k.art))
    }

    /// Takes a copy of `art` and remembers it. Keeping a kept picture does nothing.
    pub fn keep(&mut self, art: &Artwork) -> Result<()> {
        if self.holds(art) {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("making {}", self.dir.display()))?;
        let copy = self.free_name(&art.path);
        std::fs::copy(&art.path, &copy)
            .with_context(|| format!("copying {} to {}", art.path.display(), copy.display()))?;
        self.kept.push(Kept {
            origin: art.path.clone(),
            art: Artwork {
                path: copy,
                ..art.clone()
            },
        });
        self.save()
    }

    /// Drops a picture from the list. The copy itself goes at the next
    /// [`Favourites::discard_all_but`].
    pub fn forget(&mut self, key: &str) -> Result<()> {
        self.kept.retain(|k| k.key() != key);
        self.save()
    }

    /// Deletes the copies nothing claims any more, except `keep`.
    ///
    /// The same contract, and the same reason, as [`crate::art::Source::discard_all_but`]:
    /// only whoever wrote a file may decide it is rubbish, and the file on the
    /// desktop is never rubbish. That exception is what lets [`Favourites::forget`]
    /// drop the very picture on screen — the row leaves the menu at once, the file
    /// waits until the desktop is pointing somewhere else.
    pub fn discard_all_but(&self, keep: &Path) {
        let index = self.dir.join(INDEX);
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let claimed = self.kept.iter().any(|k| k.art.path == path);
            if path.is_file() && path != keep && path != index && !claimed {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// A name in the folder that nothing has taken.
    ///
    /// The original name is kept wherever it can be, because `met::id_of` reads the
    /// object id back out of it whatever folder the file sits in — which is how
    /// tomorrow's painting avoids being the favourite already on the desktop. Two
    /// pictures out of a person's own folder can share a name, though, and then one
    /// of them has to give way; a folder recognises its own work by path, so
    /// nothing is lost by it.
    fn free_name(&self, origin: &Path) -> PathBuf {
        let name = origin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "picture".to_string());
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, extension)) => (stem, format!(".{extension}")),
            None => (name.as_str(), String::new()),
        };
        let mut candidate = self.dir.join(&name);
        let mut nth = 2;
        while candidate.exists() || self.kept.iter().any(|k| k.art.path == candidate) {
            candidate = self.dir.join(format!("{stem}-{nth}{extension}"));
            nth += 1;
        }
        candidate
    }

    fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("making {}", self.dir.display()))?;
        let index = self.dir.join(INDEX);
        std::fs::write(&index, serde_json::to_string_pretty(&self.kept)?)
            .with_context(|| format!("writing {}", index.display()))
    }
}

impl Kept {
    /// How this picture is named in a menu id.
    ///
    /// The copy's file name, which is unique within the folder by construction and
    /// survives the list being reordered — so a menu rebuilt between the click and
    /// the answer still means the same painting. It is valid UTF-8 because
    /// [`Favourites::free_name`] made it so.
    fn key(&self) -> &str {
        self.art
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
    }
}
