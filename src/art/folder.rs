//! A directory of the user's own pictures.
//!
//! No network, no API, and a working fallback when a museum is unreachable.
//! The chosen file is used where it lies — copying it would duplicate a library
//! the user already curates.

use super::{pick_index, Artwork, Source};
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "heic", "tif", "tiff"];
#[cfg(not(target_os = "macos"))]
const EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "tif", "tiff"];

pub struct Folder {
    dir: PathBuf,
}

impl Folder {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Source for Folder {
    fn fetch(&self, avoid: Option<&Artwork>) -> Result<Artwork> {
        let entries = std::fs::read_dir(&self.dir)
            .with_context(|| format!("reading {}", self.dir.display()))?;

        let mut images: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                        .unwrap_or(false)
            })
            .collect();

        if images.is_empty() {
            return Err(anyhow!("no images in {}", self.dir.display()));
        }

        // Sorted so the random index means the same thing across runs on the same
        // folder; directory order is not guaranteed stable.
        images.sort();

        // A file here is recognised by where it lies, which is all the identity a
        // folder has to offer. Only worth excluding the previous pick when there is
        // something else to pick.
        if images.len() > 1 {
            if let Some(previous) = avoid {
                images.retain(|p| p != &previous.path);
            }
        }

        let path = images[pick_index(images.len(), 0)].clone();
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_owned();

        Ok(Artwork {
            title,
            byline: String::new(),
            attribution: self.dir.display().to_string(),
            details_url: None,
            path,
        })
    }

    fn label(&self) -> &'static str {
        "Folder"
    }

    /// Nothing. This source writes no files, so it owns none to throw away — and a
    /// folder of the user's own pictures is the last place to go deleting things.
    fn discard_all_but(&self, _keep: &Path) {}
}
