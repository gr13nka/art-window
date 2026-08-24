//! The Metropolitan Museum of Art's open-access collection.
//!
//! Two calls and a download: ask which European paintings are public domain and
//! photographed, pick one, then fetch its record and its image. The department
//! filter is what keeps the pool to actual paintings rather than the coins,
//! textiles and armour that dominate an unfiltered collection of 490,000 objects.
//!
//! <https://metmuseum.github.io/>

use super::{pick_index, Artwork, Source};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const API: &str = "https://collectionapi.metmuseum.org/public/collection/v1";
/// Department 11 is European Paintings.
const SEARCH: &str = "search?departmentId=11&hasImages=true&isPublicDomain=true&q=painting";

/// Refuse anything implausible for a photograph of a painting. The Met serves
/// originals with no server-side resizing, so this is the only size control there is.
const MAX_IMAGE_BYTES: u64 = 96 * 1024 * 1024;

/// How many objects to try before giving up. Records occasionally lack a usable
/// `primaryImage` despite the `hasImages` filter.
const CANDIDATES: usize = 8;

/// How long any one request may take. Generous enough for an original-resolution
/// painting on a link that has just woken up with the rest of the machine, and no
/// more, because a fetch is a chain of these rather than one of them.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// How long the whole of a fetch may take before it gives up and leaves the day for
/// the next attempt.
///
/// The tray parks its clock entirely while a fetch is in the air, so the chain has
/// to have an end: seventeen requests at two minutes each once left a menu bar
/// reading "Fetching…" for half an hour, with no tick scheduled behind it. Checked
/// between attempts rather than during one, so a request already in flight when the
/// budget runs out still gets to finish — the real ceiling is this plus one
/// [`REQUEST_TIMEOUT`].
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
}

impl Met {
    pub fn new(cache: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            // The Met asks callers to identify themselves, and the Art Institute's
            // image host demonstrated what anonymous traffic earns: a Cloudflare
            // challenge no unattended client can answer.
            .user_agent(concat!(
                "ArtWindow/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/gr13nka/art-window)"
            ))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            cache,
        }
    }

    fn candidate_ids(&self) -> Result<Vec<u64>> {
        let results: SearchResults = self
            .agent
            .get(&format!("{API}/{SEARCH}"))
            .call()
            .context("asking the Met which paintings are available")?
            .body_mut()
            .read_json()
            .context("reading the Met's list of paintings")?;

        results
            .object_ids
            .filter(|ids| !ids.is_empty())
            .ok_or_else(|| anyhow!("the Met returned no public-domain paintings"))
    }

    fn object(&self, id: u64) -> Result<Object> {
        self.agent
            .get(&format!("{API}/objects/{id}"))
            .call()
            .with_context(|| format!("fetching Met object {id}"))?
            .body_mut()
            .read_json()
            .with_context(|| format!("reading Met object {id}"))
    }

    fn download(&self, url: &str, id: u64) -> Result<PathBuf> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .with_context(|| format!("downloading {url}"))?;

        if let Some(len) = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            if len > MAX_IMAGE_BYTES {
                return Err(anyhow!(
                    "image is {len} bytes, over the {MAX_IMAGE_BYTES}-byte limit"
                ));
            }
        }

        let extension = url
            .rsplit('.')
            .next()
            .filter(|e| e.len() <= 4)
            .unwrap_or("jpg");
        // Load-bearing: `id_of` reads the object id back out of this name, which is
        // how tomorrow's painting avoids being today's.
        let path = self.cache.join(format!("met-{id}.{extension}"));

        std::fs::create_dir_all(&self.cache)
            .with_context(|| format!("creating {}", self.cache.display()))?;
        let mut file =
            std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
        let mut reader = response.body_mut().as_reader().take(MAX_IMAGE_BYTES);
        std::io::copy(&mut reader, &mut file)
            .with_context(|| format!("writing {}", path.display()))?;

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

        for attempt in 0..CANDIDATES {
            if Instant::now() >= deadline {
                // Kept only if nothing more specific went wrong: a museum that
                // refused is worth more to whoever reads the log than the clock
                // that ran out waiting for it.
                last_error
                    .get_or_insert_with(|| anyhow!("gave up after {} seconds", BUDGET.as_secs()));
                break;
            }

            let id = ids[pick_index(ids.len(), attempt as u64)];
            if Some(id) == avoid {
                continue;
            }

            let object = match self.object(id) {
                Ok(o) if !o.primary_image.is_empty() => o,
                Ok(_) => continue, // catalogued as having an image, but has none
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            match self.download(&object.primary_image, object.object_id) {
                Ok(path) => {
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
                    })
                }
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no usable painting in {CANDIDATES} attempts")))
    }

    fn label(&self) -> &'static str {
        "The Met"
    }

    /// Deletes yesterday's downloads, and only those: a file is this source's to
    /// remove exactly when `id_of` recognises its name. Anything else in the
    /// directory belongs to somebody else and is left alone.
    fn discard_all_but(&self, keep: &Path) {
        let Ok(entries) = std::fs::read_dir(&self.cache) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path != keep && id_of(&path).is_some() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
