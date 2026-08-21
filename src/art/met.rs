//! The Metropolitan Museum of Art's open-access collection.
//!
//! Two calls and a download: ask which European paintings are public domain and
//! photographed, pick one, then fetch its record and its image. The department
//! filter is what keeps the pool to actual paintings rather than the coins,
//! textiles and armour that dominate an unfiltered collection of 490,000 objects.
//!
//! <https://metmuseum.github.io/>

use super::{random_u64, Artwork, Source};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const API: &str = "https://collectionapi.metmuseum.org/public/collection/v1";
/// Department 11 is European Paintings.
const SEARCH: &str = "search?departmentId=11&hasImages=true&isPublicDomain=true&q=painting";

/// Refuse anything implausible for a photograph of a painting. The Met serves
/// originals with no server-side resizing, so this is the only size control there is.
const MAX_IMAGE_BYTES: u64 = 96 * 1024 * 1024;

/// How many objects to try before giving up. Records occasionally lack a usable
/// `primaryImage` despite the `hasImages` filter.
const CANDIDATES: usize = 8;

pub struct Met {
    agent: ureq::Agent,
    /// The object shown last time, so the same painting does not appear twice running.
    avoid: Option<u64>,
}

/// Recovers the object id from a file this source downloaded, or `None` if the
/// file came from somewhere else.
///
/// The id has to outlive the process so tomorrow's painting is not today's, and
/// the download already spells it into the filename. Remembering it a second time
/// would only create something that could disagree with the picture on screen.
pub fn id_of(path: &Path) -> Option<u64> {
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
    pub fn new(avoid: Option<u64>) -> Self {
        let config = ureq::Agent::config_builder()
            // The Met asks callers to identify themselves, and the Art Institute's
            // image host demonstrated what anonymous traffic earns: a Cloudflare
            // challenge no unattended client can answer.
            .user_agent(concat!(
                "ArtWindow/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/gr13nka/art-window)"
            ))
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            avoid,
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

    fn download(&self, url: &str, dir: &Path, id: u64) -> Result<PathBuf> {
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
        let path = dir.join(format!("met-{id}.{extension}"));

        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let mut file =
            std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
        let mut reader = response.body_mut().as_reader().take(MAX_IMAGE_BYTES);
        std::io::copy(&mut reader, &mut file)
            .with_context(|| format!("writing {}", path.display()))?;

        Ok(path)
    }
}

impl Source for Met {
    fn fetch(&self, dir: &Path) -> Result<Artwork> {
        let ids = self.candidate_ids()?;
        let mut last_error = None;

        for attempt in 0..CANDIDATES {
            let id = ids[random_u64(attempt as u64) as usize % ids.len()];
            if Some(id) == self.avoid {
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

            match self.download(&object.primary_image, dir, object.object_id) {
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
}
