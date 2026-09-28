//! Shared HTTP plumbing for sources that download a museum's own hosted images.
//!
//! [`met`](super::met) and [`museums`](super::museums) both fetch a JPEG from a
//! museum's server and nothing else; one User-Agent, one size cap and one
//! streaming download keep that single behaviour in one place rather than two
//! copies quietly drifting apart on a timeout or a size limit.

use anyhow::{anyhow, Context, Result};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// How long any one request may take. Generous enough for an original-resolution
/// painting on a link that has just woken up with the rest of the machine, and no
/// more, because a fetch is a chain of these rather than one of them.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Refuse anything implausible for a photograph of a painting. A museum serves
/// originals with no server-side resizing, so this is the only size control there
/// is.
pub(crate) const MAX_IMAGE_BYTES: u64 = 96 * 1024 * 1024;

/// A client identifying itself, with the timeout above applied to every request.
///
/// Anonymous traffic earns bot challenges — the Art Institute's image host is what
/// demonstrated it, see the "Deliberate omissions" in `CLAUDE.md` — so every source
/// asks for the same agent rather than building its own.
pub(crate) fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .user_agent(concat!(
            "ArtWindow/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/gr13nka/art-window)"
        ))
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build();
    ureq::Agent::new_with_config(config)
}

/// The extension a downloaded file should carry, guessed from the last dot in
/// `url`.
///
/// A plain filename and an IIIF `.../full/!4000,4000/0/default.jpg` both end in a
/// short, real extension, so no special case is needed for either — anything
/// longer than four characters (no dot found, or a query string in the way) falls
/// back to `jpg`, which is what every source here actually serves.
pub(crate) fn extension_from_url(url: &str) -> &str {
    url.rsplit('.')
        .next()
        .filter(|e| e.len() <= 4)
        .unwrap_or("jpg")
}

/// Downloads `url` to `dest`, capped at [`MAX_IMAGE_BYTES`].
///
/// Creates `dest`'s parent directory if needed; the cache it writes into is swept
/// daily and cannot be relied on to exist.
pub(crate) fn download(agent: &ureq::Agent, url: &str, dest: &Path) -> Result<()> {
    let mut response = agent
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

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut reader = response.body_mut().as_reader().take(MAX_IMAGE_BYTES);
    std::io::copy(&mut reader, &mut file).with_context(|| format!("writing {}", dest.display()))?;

    Ok(())
}
