//! Where settings and remembered state live.
//!
//! Two files, because they have two different authors. `config.toml` belongs to
//! the person using the program: it is read and never written, so their comments
//! and formatting survive. `state.json` belongs to the program: it is rewritten
//! freely and nobody is expected to read it.

use crate::art::{Artwork, SourceSpec};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Settings, as written by a person.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the pictures come from. The spelling of this setting belongs to
    /// [`SourceSpec`]; it is decoded while the file is read and nothing downstream
    /// ever sees the string.
    pub source: SourceSpec,
    /// How long a picture stays up before another is fetched.
    pub refresh_hours: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: SourceSpec::Met,
            refresh_hours: 24,
        }
    }
}

/// What the program remembers between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Unix seconds of the moment the day was last settled. A failure leaves this
    /// alone, so the next run retries rather than writing the day off. Private,
    /// because the only correct moments to move it are [`State::record_fetched`]
    /// and — when a picture was already owed — [`State::record_chosen`].
    last_success: Option<u64>,
    /// The picture currently on the desktop. Two jobs: the menu names it, and the
    /// next fetch avoids it so today's painting is not yesterday's.
    pub shown: Option<Artwork>,
    /// The picture the rotation last brought in — today's. Kept apart from `shown`
    /// because a picture chosen by hand takes the desktop without taking the day:
    /// this is the one the menu offers a way back to, and the one file in the cache
    /// that must survive the sweep.
    pub fetched: Option<Artwork>,
}

pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    /// The pictures kept back from the rotation, and the list naming them. A folder
    /// of its own because the cache is emptied daily and this is the one place a
    /// picture is safe from that.
    pub favourites: PathBuf,
}

impl Paths {
    pub fn locate() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "ArtWindow")
            .context("cannot determine where to keep settings")?;
        let base = dirs.data_dir().to_path_buf();
        Ok(Self {
            config: base.join("config.toml"),
            state: base.join("state.json"),
            cache: base.join("cache"),
            favourites: base.join("favourites"),
        })
    }
}

impl Config {
    /// Reads settings, falling back to defaults when the file is absent.
    ///
    /// A file that exists but cannot be parsed is an error rather than a silent
    /// reset: quietly ignoring a typo would leave someone staring at the wrong
    /// source with no idea why.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("reading {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Writes a starting `config.toml`, commented, if none exists yet.
    pub fn write_default_if_absent(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            concat!(
                "# Art Window settings.\n",
                "\n",
                "# \"met\" for public-domain paintings from the Metropolitan Museum,\n",
                "# or a path to a folder of your own pictures, e.g.\n",
                "#   source = \"/Users/you/Pictures/Wallpapers\"\n",
                "source = \"met\"\n",
                "\n",
                "# Hours a picture stays up before the next one is fetched.\n",
                "refresh_hours = 24\n",
            ),
        )
        .with_context(|| format!("writing {}", path.display()))
    }
}

impl State {
    pub fn load(path: &Path) -> Self {
        // State is a convenience, not a record of consequence. If it is missing or
        // corrupt the worst outcome is repeating a painting, so start fresh rather
        // than refusing to run.
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Records `artwork` as the picture of the day, fresh from the rotation.
    ///
    /// Stamping the clock, remembering the picture and writing the file are one
    /// operation and not three: the day is spent only by a painting that reached the
    /// desktop, and there is no way to do half of it. Callers must have hung the
    /// wallpaper first — see [`crate::rotation::show`].
    pub fn record_fetched(&mut self, artwork: &Artwork, path: &Path) -> Result<()> {
        self.last_success = Some(now_secs());
        self.fetched = Some(artwork.clone());
        self.shown = Some(artwork.clone());
        self.save(path)
    }

    /// Records `artwork` as the picture now on the desktop, chosen by hand.
    ///
    /// The day's picture is left where it is, so there is still something to come
    /// back to. The clock moves only if a picture was already owed: choosing one
    /// settles that debt, and without it the overdue fetch would start seconds later
    /// and take the desktop straight back. When nothing was owed the schedule is
    /// untouched, so tomorrow's painting arrives at its usual hour however often the
    /// desktop was changed in between.
    ///
    /// Callers must have hung the wallpaper first — see [`crate::rotation::revisit`].
    pub fn record_chosen(
        &mut self,
        artwork: &Artwork,
        refresh_hours: u64,
        path: &Path,
    ) -> Result<()> {
        if self.is_due(refresh_hours) {
            self.last_success = Some(now_secs());
        }
        self.shown = Some(artwork.clone());
        self.save(path)
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }

    /// Whether enough time has passed to warrant a new picture.
    ///
    /// A clock that has moved backwards (timezone change, NTP correction) reads as
    /// due rather than blocking refreshes until real time catches up.
    pub fn is_due(&self, refresh_hours: u64) -> bool {
        match self.last_success {
            None => true,
            Some(last) => match now_secs().checked_sub(last) {
                None => true,
                Some(elapsed) => elapsed >= refresh_hours.saturating_mul(3600),
            },
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
