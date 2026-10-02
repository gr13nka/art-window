//! A painting nobody has seen yet, kept ready as the user's virtual background in
//! the meeting app, so that every meeting opens with a new one.
//!
//! zoom.us has no API for this. It keeps the user's chosen custom background as a
//! plain PNG with a UUID for a name and no extension, in `~/Library/Application
//! Support/zoom.us/data/VirtualBkgnd_Custom/`, and shows whatever that file holds.
//! Four facts about it were established by hand on zoom.us 7.2.2, macOS 12.7, and
//! the whole design stands on them:
//!
//! 1. zoom.us re-reads the file every time video starts, and one meeting starts it
//!    twice: once for the preview before joining, and again about a second after
//!    the meeting itself begins. It needs no restart to notice a new picture.
//! 2. It does **not** re-read it once the meeting is under way. Replacing the file
//!    fifteen seconds into a call leaves the live background alone, and the next
//!    meeting shows the new picture.
//! 3. A process named exactly `CptHost` exists while a meeting is live and at no
//!    other time. `zoom.us`, `caphost` and `aomhost` run always, and the lowercase
//!    `caphost` is a different process altogether.
//! 4. The file's access time records zoom.us reading it. On APFS a read updates the
//!    access time only when it is not later than the modification time, so once we
//!    write the file with accessed older than modified, the first read flips it to
//!    accessed > modified and later reads change nothing.
//!
//! The rule this module enforces is that **the slot always holds a painting no
//! meeting has shown,** and that **one meeting shows one painting.** A painting is
//! *shown* when the slot has been read since we hung it (fact 4) and a meeting is
//! live (fact 3). The access time alone is not enough, since Time Machine and
//! Spotlight read files too; the process scan alone would be a scan every two
//! seconds for ever, so it is only asked once the access time already says "read".
//!
//! A shown painting is replaced when its meeting ends, and not before. Replacing
//! it the moment it was shown was tried first and is wrong by fact 1: the swap
//! lands between the preview's read and the meeting's, so the meeting opens with a
//! different painting from the one just previewed and spends two. What makes
//! waiting for the end cost nothing is the spare — already downloaded and
//! rendered, so replacing is one rename — and looking twice a second while a
//! meeting is live, so it is done before anyone can start the next one.
//!
//! Everything lives on one thread per app, each with one loop and no AppKit, so
//! none of it has to care which thread it is on.
//!
//! The rule is the same for every app; what differs is where the background lives,
//! what form it must be in, and what "a meeting is live" means. That is a
//! [`Stage`], one per app in its own submodule: [`zoom_us`] and [`meet_firefox`].
//! The worker below is written once and knows none of it. Each enabled app has a
//! worker, a spare and a cache directory of its own, so two apps simply show
//! different paintings and share nothing.

use crate::art::{self, Artwork, Selection};
use crate::config::{now_secs, Config};
use crate::settings::{Filters, Shape};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, FileTimes, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

mod meet_firefox;
mod probe;
mod zoom_us;

/// How often the slot is looked at while nothing is happening. The look itself is
/// one `stat`, and all it has to catch is a meeting starting.
const POLL: Duration = Duration::from_secs(2);
/// How often a live meeting is checked for having ended. Much finer, because the
/// gap it has to beat is a person ending one meeting and starting the next.
const WATCH: Duration = Duration::from_millis(500);
/// How long a failure leaves things alone, in wall-clock seconds, for the reason
/// every deadline in `tray.rs` is one: `Instant` does not advance with the lid shut.
const COOLING_OFF: u64 = 15 * 60;
/// How much older than its modification the access time is set when a painting is
/// hung. Only the order matters; a minute leaves no doubt about it.
const ARMED_BY: Duration = Duration::from_secs(60);
/// The camera frame, which is what a virtual background is cropped to — not the
/// display.
const FRAME: (u32, u32) = (1920, 1080);

/// A meeting app whose background this program can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum App {
    ZoomUs,
    MeetFirefox,
}

impl App {
    /// The apps reachable on this platform, in menu order. Empty off macOS.
    pub fn all() -> &'static [App] {
        if cfg!(target_os = "macos") {
            &[App::ZoomUs, App::MeetFirefox]
        } else {
            &[]
        }
    }

    /// The menu row.
    pub fn label(self) -> &'static str {
        match self {
            App::ZoomUs => "Paintings in Zoom meetings",
            App::MeetFirefox => "Paintings in Google Meet (Firefox)",
        }
    }

    /// The app's own directory under `<cache>/backdrop/`.
    fn directory(self) -> &'static str {
        match self {
            App::ZoomUs => "zoom_us",
            App::MeetFirefox => "meet_firefox",
        }
    }
}

/// Where one app keeps the background this program may overwrite. Opaque: it is
/// serialised into `state.json` and handed back on the next run, and only the
/// submodule that made it knows what the paths in it mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slot {
    place: Place,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "app", rename_all = "snake_case")]
enum Place {
    ZoomUs(zoom_us::ZoomUs),
    MeetFirefox(meet_firefox::MeetFirefox),
}

impl Slot {
    /// Finds the background the user has added in `app`.
    ///
    /// Errors, in words fit for a menu status line, when there is none: the user
    /// has to add a picture as their background in the app first, because that is
    /// the only way a file this program may overwrite comes to exist.
    pub fn adopt(app: App) -> Result<Slot> {
        let place = match app {
            App::ZoomUs => Place::ZoomUs(zoom_us::ZoomUs::adopt()?),
            App::MeetFirefox => Place::MeetFirefox(meet_firefox::MeetFirefox::adopt()?),
        };
        Ok(Slot { place })
    }

    pub fn app(&self) -> App {
        match self.place {
            Place::ZoomUs(_) => App::ZoomUs,
            Place::MeetFirefox(_) => App::MeetFirefox,
        }
    }

    fn into_stage(self) -> Box<dyn Stage> {
        match self.place {
            Place::ZoomUs(stage) => Box::new(stage),
            Place::MeetFirefox(stage) => Box::new(stage),
        }
    }
}

/// Everything that differs between meeting apps. The worker owns the rule and the
/// timing; a stage answers four questions about one app.
trait Stage: Send {
    /// Writes `painting`, in the form this app wants, into `spare`, a directory
    /// that exists. What it holds afterwards is for [`Stage::hang`] alone to read.
    fn render(&self, painting: &Path, spare: &Path) -> Result<()>;

    /// Moves what [`Stage::render`] left in `spare` into place and arms the access
    /// time. Fails with [`Stale`] when the app's background is no longer the one
    /// the spare was rendered for.
    fn hang(&self, spare: &Path) -> Result<()>;

    /// Whether anything has read the background since [`Stage::hang`] armed it. Any
    /// error reads as no: a file that cannot be examined is not one to hand over.
    fn read_since_hung(&self) -> bool;

    /// Whether a meeting is live. Asked only once the background has been read,
    /// so nothing is scanned while the app sits idle.
    fn live(&self) -> bool;
}

/// The spare no longer fits where it was going: the user changed their background
/// in the app. Unlike a deleted file this is cured by rendering again, so the
/// worker throws the spare away rather than offering it again in fifteen minutes.
#[derive(Debug)]
struct Stale(String);

impl std::fmt::Display for Stale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Stale {}

/// Whether `path` has been read since [`arm`] set it: accessed later than
/// modified.
fn read_since_armed(path: &Path) -> bool {
    let read = || -> std::io::Result<bool> {
        let meta = fs::metadata(path)?;
        Ok(meta.accessed()? > meta.modified()?)
    };
    read().unwrap_or(false)
}

/// Sets `path`'s modification time to now and its access time a minute earlier, so
/// that the first read after this is visible in [`read_since_armed`].
fn arm(path: &Path) -> Result<()> {
    let now = SystemTime::now();
    OpenOptions::new()
        .write(true)
        .open(path)?
        .set_times(
            FileTimes::new()
                .set_modified(now)
                .set_accessed(now - ARMED_BY),
        )
        .with_context(|| format!("setting the times of {}", path.display()))
}

/// Puts `from` at `to`: a rename means the app never reads half a picture, and
/// only when that fails, across volumes, is it copied.
fn move_into_place(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_err() {
        fs::copy(from, to).with_context(|| format!("copying into {}", to.display()))?;
    }
    Ok(())
}

/// One app's running feature. Dropping it stops it.
pub struct Backdrop {
    /// The thread's only inbox. Its being dropped is how the thread is told to end.
    filters: Sender<Filters>,
}

impl Backdrop {
    /// Starts the thread. `fill_now` is true when the slot has just been adopted
    /// and still holds the user's own picture: the first painting ready goes
    /// straight in without waiting to be shown. False on a restart, when the slot
    /// already holds a painting.
    pub fn begin(
        slot: Slot,
        fill_now: bool,
        config: &Config,
        cache: &Path,
        filters: &Filters,
    ) -> Backdrop {
        let (sender, inbox) = mpsc::channel();
        let cache = cache.join("backdrop").join(slot.app().directory());
        let stage = slot.into_stage();
        // A painting read while this program was not running was most likely shown
        // by a meeting nobody was here to see end. Counting it as shown costs one
        // painting if that guess is wrong; not counting it shows a meeting a
        // painting twice.
        let shown = !fill_now && stage.read_since_hung();
        let mut worker = Worker {
            stage,
            fill_now,
            config: config.clone(),
            cache,
            filters: filters.clone(),
            spare: None,
            last: None,
            shown,
            retry_at: 0,
        };
        thread::spawn(move || loop {
            let pace = if worker.shown { WATCH } else { POLL };
            match inbox.recv_timeout(pace) {
                Ok(filters) => worker.filters = filters,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            worker.tick();
        });
        Backdrop { filters: sender }
    }

    /// The settings window changed the filters; later paintings follow them.
    pub fn set_filters(&self, filters: &Filters) {
        // A send fails only once the thread has gone, and then nobody is waiting.
        let _ = self.filters.send(filters.clone());
    }
}

/// What the thread owns.
struct Worker {
    stage: Box<dyn Stage>,
    fill_now: bool,
    config: Config,
    /// `<cache>/backdrop/<app>`: a directory of its own, so the source downloads and
    /// tidies here and the wallpaper's sweep never sees a backdrop download. This
    /// can never delete today's wallpaper.
    cache: PathBuf,
    filters: Filters,
    /// The painting waiting, rendered into [`Worker::spare_dir`].
    spare: Option<Artwork>,
    /// The last one hung, in memory only, so tomorrow's is not a repeat.
    last: Option<Artwork>,
    /// A meeting has shown the painting in the slot, and may be showing it still.
    shown: bool,
    /// Wall-clock seconds before which a failure is not tried again.
    retry_at: u64,
}

impl Worker {
    fn tick(&mut self) {
        let cooling = now_secs() < self.retry_at;
        if self.spare.is_none() && !cooling {
            // Downloading blocks this loop for as long as the source takes, and
            // that is acceptable: it happens right after a swap, so the slot
            // already holds a painting nobody has seen, and the next swap cannot
            // matter before a whole meeting has come and gone.
            match self.prepare() {
                Ok(spare) => self.spare = Some(spare),
                Err(e) => self.fail(&e),
            }
        }
        // Liveness is only asked about once the access time says "read", so
        // nothing is scanned while the app sits idle.
        if !self.shown {
            self.shown = self.stage.read_since_hung() && self.stage.live();
        }
        // Not while the meeting runs: the app may read the file a second time just
        // after it begins, and a swap before that would open it with another
        // painting.
        let over = self.shown && !self.stage.live();
        if (over || self.fill_now) && self.spare.is_some() && !cooling {
            match self.stage.hang(&self.spare_dir()) {
                Ok(()) => {
                    self.last = self.spare.take();
                    self.shown = false;
                    self.fill_now = false;
                }
                Err(e) => {
                    if e.is::<Stale>() {
                        self.spare = None;
                    }
                    self.fail(&e);
                }
            }
        }
    }

    /// Without a cooling-off a dead network, or a slot that has been deleted,
    /// would be tried again every two seconds for ever.
    fn fail(&mut self, error: &anyhow::Error) {
        eprintln!("art-window: {error:#}");
        self.retry_at = now_secs() + COOLING_OFF;
    }

    fn spare_dir(&self) -> PathBuf {
        self.cache.join("spare")
    }

    /// Fetches a painting and has the stage render it, at the camera's shape, into
    /// [`Worker::spare_dir`].
    fn prepare(&self) -> Result<Artwork> {
        let spare = self.spare_dir();
        fs::create_dir_all(&spare).with_context(|| format!("creating {}", spare.display()))?;
        let selection = Selection {
            filters: Filters {
                shape: Shape::Screen,
                ..self.filters.clone()
            },
            screen_aspect: f64::from(FRAME.0) / f64::from(FRAME.1),
        };
        let source = art::source_for(&self.config.source, &self.cache, &selection);
        let artwork = source.fetch(self.last.as_ref()).and_then(|artwork| {
            self.stage.render(&artwork.path, &spare)?;
            Ok(artwork)
        });
        // The rendered copy is all that is wanted now, or nothing is, if the
        // render failed. A `Folder` source implements this as nothing, so a
        // person's own pictures are safe.
        source.discard_all_but(Path::new(""));
        artwork
    }
}

/// A directory of one's own under the temp dir, empty.
#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("art-window-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `bytes` to `path` and sets its modification time, leaving the access time
/// where the write put it.
#[cfg(test)]
fn write_at(path: &Path, bytes: &[u8], modified: SystemTime) {
    fs::write(path, bytes).unwrap();
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(modified))
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_survives_state_json_for_both_apps() {
        let slots = [
            Slot {
                place: Place::ZoomUs(zoom_us::ZoomUs::at(PathBuf::from("/z/uuid"))),
            },
            Slot {
                place: Place::MeetFirefox(meet_firefox::MeetFirefox::at(
                    PathBuf::from("/f/3"),
                    PathBuf::from("/f/4"),
                )),
            },
        ];
        for slot in slots {
            let text = serde_json::to_string(&slot).unwrap();
            assert_eq!(serde_json::from_str::<Slot>(&text).unwrap(), slot);
        }
    }
}
