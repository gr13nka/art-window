//! The Metropolitan Museum of Art's open-access collection.
//!
//! Two calls and a download: ask which European landscape paintings are public
//! domain and photographed, pick one, then fetch its record and its image. The department
//! filter is what keeps the pool to actual paintings rather than the coins,
//! textiles and armour that dominate an unfiltered collection of 490,000 objects.
//!
//! <https://metmuseum.github.io/>
//!
//! No other platform searches the Met live any more — the phone apps draw only
//! from the prebuilt catalogue — so the search query and the `User-Agent` are
//! this file's alone. The `met-{id}.{ext}` filename is still read by Android's
//! `Museums.kt`, as the name of a download made before the catalogue existed.

use super::http;
use super::{file_name as name, pick_index, Artwork, Source};
use crate::journal;
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const API: &str = "https://collectionapi.metmuseum.org/public/collection/v1";
const SEARCH_API: &str = "https://collectionapi.metmuseum.org/public/collection/v1.1/search";
/// Department 11 is European Paintings.
// A generic `q=painting` makes portraits disproportionately common in the European
// Paintings department. Start from a pool of landscapes instead — the subject, not
// the shape of the canvas; the catalogue metadata check below catches the portraits
// that happen to mention a landscape too.
const SEARCH: &str = "departmentId=11&hasImages=true&isPublicDomain=true&q=landscape";
const SEARCH_PAGE: usize = 500;
const MAX_SEARCH_RESULTS: usize = 10_000;

/// How many objects to try before giving up. Records occasionally lack a usable
/// `primaryImage` despite the `hasImages` filter.
const MAX_ATTEMPTS: usize = 8;

/// How long the whole of a fetch may take before it gives up and leaves the day for
/// the next attempt.
///
/// The tray parks its clock entirely while a fetch is in the air, so the chain has
/// to have an end: seventeen requests at two minutes each once left a menu bar
/// reading "Fetching…" for half an hour, with no tick scheduled behind it. Checked
/// between attempts rather than during one, so a request already in flight when the
/// budget runs out still gets to finish — the real ceiling is this plus one
/// [`http::REQUEST_TIMEOUT`].
const BUDGET: Duration = Duration::from_secs(90);

pub struct Met {
    agent: ureq::Agent,
    /// Where downloads go, and the only directory this source will delete from.
    cache: PathBuf,
}

/// Recovers the object id from a file this source downloaded, or `None` if the
/// file came from somewhere else.
///
/// The id has to outlive the process so tomorrow's painting is not today's, and
/// the download already spells it into the filename. Remembering it a second time
/// would only create something that could disagree with the picture on screen.
///
/// Private, and the reason `fetch` and `discard_all_but` take whole paths rather
/// than ids: recognising this source's own work is exactly the knowledge that has
/// no business leaving this file.
fn id_of(path: &Path) -> Option<u64> {
    path.file_stem()?
        .to_str()?
        .strip_prefix("met-")?
        .parse()
        .ok()
}

#[derive(Deserialize)]
struct SearchResults {
    total: usize,
    #[serde(rename = "objectIDs")]
    object_ids: Option<Vec<u64>>,
}

#[derive(Deserialize)]
struct Object {
    #[serde(rename = "objectID")]
    object_id: u64,
    title: String,
    #[serde(rename = "artistDisplayName")]
    artist: String,
    #[serde(rename = "objectDate")]
    date: String,
    #[serde(rename = "primaryImage")]
    primary_image: String,
    #[serde(rename = "objectURL")]
    object_url: String,
    #[serde(default)]
    tags: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    term: String,
}

impl Object {
    fn is_portrait(&self) -> bool {
        let title = self.title.to_ascii_lowercase();
        title.contains("portrait")
            || self
                .tags
                .iter()
                .any(|tag| tag.term.eq_ignore_ascii_case("portraits"))
    }
}

impl Met {
    pub fn new(cache: PathBuf) -> Self {
        Self {
            agent: http::agent(),
            cache,
        }
    }

    fn candidate_ids(&self) -> Result<Vec<u64>> {
        let mut ids = Vec::new();
        let mut offset = 0;
        loop {
            let results: SearchResults = http::get_json(
                &self.agent,
                &format!("{SEARCH_API}?{SEARCH}&limit={SEARCH_PAGE}&offset={offset}"),
                "the Met's list of paintings",
            )?;
            let total = results.total.min(MAX_SEARCH_RESULTS);
            let page = results.object_ids.unwrap_or_default();
            if page.is_empty() {
                break;
            }
            ids.extend(page);
            offset += SEARCH_PAGE;
            if offset >= total {
                break;
            }
        }

        if ids.is_empty() {
            Err(anyhow!("the Met returned no public-domain paintings"))
        } else {
            journal::note!("fetch", "met: {} candidate paintings", ids.len());
            Ok(ids)
        }
    }

    fn object(&self, id: u64) -> Result<Object> {
        http::get_json(
            &self.agent,
            &format!("{API}/objects/{id}"),
            &format!("Met object {id}"),
        )
    }

    fn download(&self, url: &str, id: u64) -> Result<PathBuf> {
        let extension = http::extension_from_url(url);
        // Load-bearing: `id_of` reads the object id back out of this name, which is
        // how tomorrow's painting avoids being today's.
        let path = self.cache.join(format!("met-{id}.{extension}"));
        http::download(&self.agent, url, &path)?;
        Ok(path)
    }
}

impl Source for Met {
    fn fetch(&self, avoid: Option<&Artwork>) -> Result<Artwork> {
        // The previous picture arrives whole and is read for an id here, where the
        // filename convention that carries it is already known.
        let avoid = avoid.and_then(|a| id_of(&a.path));
        let ids = self.candidate_ids()?;
        let mut last_error = None;
        let deadline = Instant::now() + BUDGET;

        for attempt in 0..MAX_ATTEMPTS {
            if Instant::now() >= deadline {
                // Kept only if nothing more specific went wrong: a museum that
                // refused is worth more to whoever reads the log than the clock
                // that ran out waiting for it.
                journal::note!(
                    "fetch",
                    "met: over the {} second budget before attempt {}",
                    BUDGET.as_secs(),
                    attempt + 1
                );
                last_error
                    .get_or_insert_with(|| anyhow!("gave up after {} seconds", BUDGET.as_secs()));
                break;
            }

            let id = ids[pick_index(ids.len(), attempt as u64)];
            if Some(id) == avoid {
                journal::note!("fetch", "met: skipped {id}, already on the desktop");
                continue;
            }

            let object = match self.object(id) {
                Ok(o) if !o.primary_image.is_empty() && !o.is_portrait() => o,
                Ok(o) => {
                    // No usable image, or catalogued as a portrait.
                    let why = if o.primary_image.is_empty() {
                        "no image"
                    } else {
                        "a portrait"
                    };
                    journal::note!("fetch", "met: skipped {id}, {why}");
                    continue;
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            match self.download(&object.primary_image, object.object_id) {
                Ok(path) => {
                    journal::note!(
                        "fetch",
                        "met: chose {}, {:?}, file {}",
                        object.object_id,
                        object.title,
                        name(&path)
                    );
                    return Ok(Artwork {
                        byline: match (object.artist.trim(), object.date.trim()) {
                            ("", "") => String::new(),
                            ("", d) => d.to_owned(),
                            (a, "") => a.to_owned(),
                            (a, d) => format!("{a}, {d}"),
                        },
                        title: if object.title.trim().is_empty() {
                            "Untitled".to_owned()
                        } else {
                            object.title
                        },
                        attribution: "The Metropolitan Museum of Art".to_owned(),
                        details_url: Some(object.object_url),
                        path,
                    });
                }
                Err(e) => {
                    journal::note!("fetch", "met: download of {id} failed, trying another");
                    last_error = Some(e)
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no usable painting in {MAX_ATTEMPTS} attempts")))
    }

    fn label(&self) -> &'static str {
        "The Met"
    }

    /// Deletes yesterday's downloads, and only those: a file is this source's to
    /// remove exactly when `id_of` recognises its name. Anything else in the
    /// directory belongs to somebody else and is left alone.
    fn discard_all_but(&self, keep: Option<&Path>) {
        let Ok(entries) = std::fs::read_dir(&self.cache) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && Some(path.as_path()) != keep && id_of(&path).is_some() {
                match std::fs::remove_file(&path) {
                    Ok(()) => journal::note!("sweep", "met: deleted {}", name(&path)),
                    Err(e) => journal::note!("sweep", "met: could not delete {}: {e}", name(&path)),
                }
            }
        }
    }
}
