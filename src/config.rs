//! Where settings and remembered state live.
//!
//! Two files, because they have two different authors. `config.toml` belongs to
//! the person using the program: it is read and never written, so their comments
//! and formatting survive. `state.json` belongs to the program: it is rewritten
//! freely and nobody is expected to read it.

use crate::art::{Artwork, SourceSpec};
use crate::backdrop::{App, Slot};
use crate::day;
use crate::journal;
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
    /// Retired: the schedule is a calendar day now and has no knob to turn. Still
    /// parsed, and thrown away, because `deny_unknown_fields` would otherwise turn
    /// every `config.toml` written by an earlier version into a startup failure —
    /// and one that happens before there is a menu bar to report it in.
    refresh_hours: serde::de::IgnoredAny,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: SourceSpec::Museums,
            refresh_hours: serde::de::IgnoredAny,
        }
    }
}

/// What the program remembers between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Unix seconds of the moment the day was last settled — kept as an instant
    /// rather than a date because [`day::local`] can read the day back out of it,
    /// while a date could not answer how long ago it was. A failure leaves this
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
    /// Where each meeting app keeps the background that paintings are written
    /// into, and so also which apps they are wanted in at all: there is no
    /// separate switch that could come to disagree with it. At most one per app.
    /// See [`crate::backdrop`].
    pub backdrops: Vec<Slot>,
}

pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    /// What the settings window chose — see [`crate::settings`].
    pub settings: PathBuf,
    /// Where the day's download lands. Genuinely disposable: the source empties it
    /// on every rotation, so it belongs wherever the platform puts things it would
    /// not mind losing.
    pub cache: PathBuf,
    /// The pictures kept back from the rotation, and the list naming them. A folder
    /// of its own because the cache is emptied daily and this is the one place a
    /// picture is safe from that.
    pub favourites: PathBuf,
    /// What the program did and when — see [`crate::journal`].
    pub log: PathBuf,
}

impl Paths {
    /// Each file where the platform keeps that kind of thing.
    ///
    /// Three directories rather than one, because the four files are three kinds:
    /// something a person wrote, something the program remembers, and something it
    /// can lose. On Linux those are three separate XDG directories and a user would
    /// be surprised to find any of them in the others. On macOS `config_dir` and
    /// `data_dir` are the same Application Support folder, so only the cache moves
    /// out — which it can afford to, being swept on every rotation anyway.
    pub fn locate() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "ArtWindow")
            .context("cannot determine where to keep settings")?;
        Ok(Self {
            config: dirs.config_dir().join("config.toml"),
            state: dirs.data_dir().join("state.json"),
            settings: dirs.data_dir().join("settings.json"),
            cache: dirs.cache_dir().to_path_buf(),
            favourites: dirs.data_dir().join("favourites"),
            log: log_file(&dirs),
        })
    }
}

/// On macOS the journal is the file the launch agent already sends stderr to —
/// see `desktop/macos/login.rs` — so there is one file to read however the program
/// was started. `~/Library/Logs` is also where Console looks.
#[cfg(target_os = "macos")]
fn log_file(dirs: &directories::ProjectDirs) -> PathBuf {
    match directories::BaseDirs::new() {
        Some(base) => base.home_dir().join("Library/Logs/ArtWindow.log"),
        None => dirs.data_dir().join("art-window.log"),
    }
}

/// Beside the state file: on Windows that is where stderr is already redirected.
#[cfg(not(target_os = "macos"))]
fn log_file(dirs: &directories::ProjectDirs) -> PathBuf {
    dirs.data_dir().join("art-window.log")
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
                "# \"museums\" for public-domain paintings from the open collections\n",
                "# of six museums, the Met among them, \"met\" to search\n",
                "# the Metropolitan Museum's collection live instead, or a path to a\n",
                "# folder of your own pictures, e.g.\n",
                "#   source = \"~/Pictures/Wallpapers\"\n",
                "source = \"museums\"\n",
            ),
        )
        .with_context(|| format!("writing {}", path.display()))
    }
}

impl State {
    pub fn load(path: &Path) -> Self {
        // Starting fresh rather than refusing to run: a painting may come round
        // again and the meeting backgrounds are switched off until asked for again,
        // neither of which is worth a program that will not start. Missing is the
        // ordinary first run; unreadable is said aloud, because the next save writes
        // over the only evidence of it.
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    journal::note!("state", "no state file, starting fresh");
                } else {
                    journal::note!(
                        "state",
                        "FAILED: reading {}: {e}; starting fresh",
                        path.display()
                    );
                }
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(state) => {
                journal::note!("state", "loaded, {}", state.summary());
                state
            }
            Err(e) => {
                journal::note!(
                    "state",
                    "FAILED: {} will not parse ({e}); starting fresh",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Records `artwork` as the picture of the day, fresh from the rotation.
    ///
    /// Stamping the clock, remembering the picture and writing the file are one
    /// operation and not three: the day is spent only by a painting that reached the
    /// desktop, and there is no way to do half of it. Callers must have hung the
    /// wallpaper first — see [`crate::rotation::show`].
    pub fn record_fetched(&mut self, artwork: &Artwork, path: &Path) -> Result<()> {
        journal::note!(
            "state",
            "day settled by a fetched picture, {}",
            name(artwork)
        );
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
    pub fn record_chosen(&mut self, artwork: &Artwork, path: &Path) -> Result<()> {
        let owed = self.is_due();
        journal::note!(
            "state",
            "chose {}, day {}",
            name(artwork),
            if owed {
                "settled by it, a picture was owed"
            } else {
                "left alone"
            }
        );
        if owed {
            self.last_success = Some(now_secs());
        }
        self.shown = Some(artwork.clone());
        self.save(path)
    }

    /// Records where `app`'s meeting backgrounds go, or with `None` that they
    /// have been switched off there.
    ///
    /// The clock and both pictures are left alone: a meeting background takes
    /// neither the desktop nor the day.
    pub fn record_backdrop(&mut self, app: App, slot: Option<Slot>, path: &Path) -> Result<()> {
        journal::note!(
            "state",
            "backdrop slot for {app:?}: {}",
            if slot.is_some() { "set" } else { "cleared" }
        );
        self.backdrops.retain(|kept| kept.app() != app);
        self.backdrops.extend(slot);
        self.save(path)
    }

    /// Written beside itself and renamed into place, so a crash or a full disk
    /// part-way through leaves the last good file rather than half of a new one —
    /// which [`State::load`] would read as no state at all.
    /// The day's own picture, when there is a way back to it: one was fetched, the
    /// desktop is showing something else, and the file is still there.
    ///
    /// The file is checked for because a source can be changed, or a cache emptied,
    /// between the picture being fetched and anyone asking for it again; an offer
    /// that can only fail is worse than no offer.
    pub fn way_back(&self) -> Option<&Artwork> {
        self.fetched
            .as_ref()
            .filter(|art| Some(&art.path) != self.shown.as_ref().map(|shown| &shown.path))
            .filter(|art| art.path.exists())
    }

    /// The state in a line, for the journal: what is on the desktop, what the day
    /// brought, and what the calendar makes of the two.
    pub fn summary(&self) -> String {
        let name = |art: &Option<Artwork>| match art {
            Some(art) => art.path.display().to_string(),
            None => "nothing".to_owned(),
        };
        let settled = match self.last_success {
            Some(at) => format!("day {}", day::local(at)),
            None => "never".to_owned(),
        };
        format!(
            "shown {}, fetched {}, settled {settled}, today is day {}, due {}",
            name(&self.shown),
            name(&self.fetched),
            day::local(now_secs()),
            self.is_due(),
        )
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let unfinished = path.with_extension("json.tmp");
        let saved = std::fs::write(&unfinished, serde_json::to_string_pretty(self)?)
            .and_then(|()| std::fs::rename(&unfinished, path))
            .with_context(|| format!("writing {}", path.display()));
        match &saved {
            Ok(()) => journal::note!("state", "saved"),
            Err(error) => journal::fault("state", error),
        }
        saved
    }

    /// Whether the local date has changed since the last picture was settled.
    ///
    /// A calendar day and not an elapsed interval, because an interval anchored to
    /// the last success drifts: a machine asleep past the appointed moment settles
    /// the day whenever it wakes, and that becomes the new anchor. Left long enough
    /// the changeover walks right around the clock, and "a new day, a new painting"
    /// stops being true in the only sense anyone means it.
    ///
    /// Compared for difference rather than for order, so a clock that has moved
    /// backwards (timezone change, NTP correction) reads as due rather than
    /// stopping the rotation until real time catches up.
    pub fn is_due(&self) -> bool {
        match self.last_success {
            None => true,
            Some(last) => day::local(now_secs()) != day::local(last),
        }
    }
}

/// The picture's file name, which is all the journal says of it.
fn name(artwork: &Artwork) -> &str {
    crate::art::file_name(&artwork.path)
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_replaces_the_file_whole_and_leaves_nothing_beside_it() {
        let dir = scratch("state-save");
        let path = dir.join("state.json");
        std::fs::write(&path, "left over from before").unwrap();

        let mut state = State::default();
        state.record_backdrop(App::ZoomUs, None, &path).unwrap();

        assert!(serde_json::from_str::<State>(&std::fs::read_to_string(&path).unwrap()).is_ok());
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "only state.json should remain");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_or_unreadable_state_starts_fresh() {
        let dir = scratch("state-load");
        let path = dir.join("state.json");
        assert!(State::load(&path).is_due());

        std::fs::write(&path, "{ \"shown\": ").unwrap();
        let state = State::load(&path);
        assert!(state.shown.is_none());
        assert!(state.backdrops.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
