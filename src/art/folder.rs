//! A directory of the user's own pictures.
//!
//! No network, no API, and a working fallback when a museum is unreachable.
//! The chosen file is used where it lies — copying it would duplicate a library
//! the user already curates.

use super::{random_u64, Artwork, Source};
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

const EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "heic", "tif", "tiff"];

pub struct Folder {
    dir: PathBuf,
    /// The file shown last time, so a small folder does not repeat immediately.
    avoid: Option<PathBuf>,
}

impl Folder {
    pub fn new(dir: PathBuf, avoid: Option<PathBuf>) -> Self {
        Self { dir, avoid }
    }
}

impl Source for Folder {
    /// Ignores `_cache`: the picture is already on disk and stays where it is.
    fn fetch(&self, _cache: &Path) -> Result<Artwork> {
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

        // Only worth excluding the previous pick when there is something else to pick.
        if images.len() > 1 {
            if let Some(avoid) = &self.avoid {
                images.retain(|p| p != avoid);
            }
        }

        let path = images[random_u64(0) as usize % images.len()].clone();
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
}
