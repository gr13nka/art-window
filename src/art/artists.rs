//! The painters the catalogue names, each with one picture to know them by.
//!
//! A name on a chip says nothing to someone who has not heard it, so the settings
//! window shows a painter the way it shows a favourite: by a painting.
//! `catalogue/build.py` picks that painting — the `showcase` in
//! `catalogue/artists.json` — saves it at a size fit for looking at, and writes
//! `catalogue/dist/artists/index.tsv` beside it; `build.rs` compiles all of it in, so
//! opening the browser downloads nothing and works with no connection at all.
//!
//! Who *may be chosen* is still [`museums`](super::museums)'s to say, from the
//! paintings themselves. This module only knows what a painter looks like and
//! where to read about them.

use crate::settings::Region;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// `INDEX`, the text of `index.tsv`, and `PICTURES`, each file it names with its
// bytes — written by `build.rs` from whatever `catalogue/dist/artists/` holds.
include!(concat!(env!("OUT_DIR"), "/artists.rs"));

pub struct Artist {
    /// Spelled exactly as the catalogue's artist column spells it, which is what
    /// a filter holds and what [`museums::artists`](super::museums::artists) lists.
    pub name: String,
    pub region: Region,
    /// Where to read about them.
    pub about: String,
    /// The painting they are shown by, and its "painter, year" as the catalogue
    /// has it.
    pub title: String,
    pub byline: String,
    file: String,
}

/// Every painter with a picture, in the order the build wrote them.
pub fn all() -> &'static [Artist] {
    static ARTISTS: OnceLock<Vec<Artist>> = OnceLock::new();
    ARTISTS.get_or_init(|| parse(INDEX))
}

/// A malformed row is skipped rather than reported, for the reason a malformed
/// catalogue row is: the file is generated, so the fault is the pipeline's.
fn parse(text: &str) -> Vec<Artist> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let [name, region, about, _showcase, title, byline, file] = fields[..] else {
                return None;
            };
            Some(Artist {
                name: name.to_owned(),
                region: Region::parse_tsv(region)?,
                about: about.to_owned(),
                title: title.to_owned(),
                byline: byline.to_owned(),
                file: file.to_owned(),
            })
        })
        .collect()
}

/// The painter's picture as a file in `dir`, written there the first time it is
/// asked for.
///
/// A file and not bytes, because every window already makes its thumbnails and
/// its large preview from a path, and a second way of decoding would have to be
/// written three times — twice for platforms nobody can run from the development
/// machine. The `artist-` prefix is nobody's download: neither source's sweep
/// recognises it, so it stays put while paintings come and go around it.
pub fn picture(artist: &Artist, dir: &Path) -> Result<PathBuf> {
    let bytes = PICTURES
        .iter()
        .find_map(|(file, bytes)| (*file == artist.file).then_some(*bytes))
        .ok_or_else(|| anyhow!("no picture was built in for {}", artist.name))?;
    let path = dir.join(format!("artist-{}", artist.file));
    // The length is the whole check: a picture changes only with a new build of
    // the catalogue, and a file cut short by a full disk is the case to catch.
    let already = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
    if !already {
        std::fs::create_dir_all(dir)?;
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_read_and_a_broken_one_skipped() {
        let artists = parse(
            "# Generated.\n\
             Tom Roberts\tOCEANIA\thttps://en.wikipedia.org/wiki/Tom_Roberts\t22142609\tShearing the rams\tTom Roberts, 1890\ttom-roberts.jpg\n\
             Nobody\tATLANTIS\thttps://example.org\t1\tUntitled\tNobody\tnobody.jpg\n\
             Too\tfew\tcolumns\n",
        );
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].name, "Tom Roberts");
        assert_eq!(artists[0].region, Region::Oceania);
        assert_eq!(artists[0].title, "Shearing the rams");
    }

    #[test]
    fn every_painter_listed_has_a_picture_built_in() {
        for artist in all() {
            assert!(
                PICTURES.iter().any(|(file, _)| *file == artist.file),
                "{} names {} which was not compiled in",
                artist.name,
                artist.file
            );
        }
    }

    #[test]
    fn every_painter_who_can_be_chosen_can_be_shown() {
        for name in crate::art::museums::artists() {
            assert!(
                all().iter().any(|a| &a.name == name),
                "{name} is in the catalogue's artist column but not in catalogue/dist/artists/index.tsv"
            );
        }
    }
}
