//! zoom.us: the background is one file, replaced whole.
//!
//! zoom.us keeps the user's chosen custom background as a plain PNG with a UUID for
//! a name and no extension, in `~/Library/Application Support/zoom.us/data/
//! VirtualBkgnd_Custom/`, and shows whatever that file holds. Four facts about it
//! were established by hand on zoom.us 7.2.2, macOS 12.7, and the whole design
//! stands on them:
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

use super::{arm, move_into_place, probe, read_since_armed, Stage, FRAME};
use crate::placement;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The rendered picture's name inside the spare directory.
const SPARE: &str = "next.png";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomUs {
    path: PathBuf,
}

impl ZoomUs {
    /// The newest file (by modification time) in zoom.us's `VirtualBkgnd_Custom`
    /// folder.
    pub fn adopt() -> Result<ZoomUs> {
        match backgrounds_folder() {
            Some(dir) => Self::adopt_from(&dir),
            None => bail!("Virtual backgrounds are not supported on this system"),
        }
    }

    fn adopt_from(dir: &Path) -> Result<ZoomUs> {
        let newest = fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let meta = entry.metadata().ok()?;
                meta.is_file()
                    .then_some((meta.modified().ok()?, entry.path()))
            })
            .max_by_key(|(modified, _)| *modified);
        match newest {
            Some((_, path)) => Ok(ZoomUs { path }),
            None => bail!("Add a picture as your virtual background in zoom.us first"),
        }
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> ZoomUs {
        ZoomUs { path }
    }
}

impl Stage for ZoomUs {
    fn render(&self, painting: &Path, spare: &Path) -> Result<()> {
        placement::cover_to(painting, FRAME, &spare.join(SPARE))
    }

    /// The file must already be there: if the user deleted that background in
    /// zoom.us, recreating it would leave a file the app no longer knows about, so
    /// this is an error instead.
    fn hang(&self, spare: &Path) -> Result<()> {
        if !self.path.is_file() {
            bail!(
                "{} is gone; the background was removed in zoom.us",
                self.path.display()
            );
        }
        move_into_place(&spare.join(SPARE), &self.path)?;
        arm(&self.path)
    }

    fn read_since_hung(&self) -> bool {
        read_since_armed(&self.path)
    }

    fn live(&self) -> bool {
        probe::meeting_process()
    }
}

#[cfg(target_os = "macos")]
fn backgrounds_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/zoom.us/data/VirtualBkgnd_Custom"))
}

#[cfg(not(target_os = "macos"))]
fn backgrounds_folder() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::super::{scratch, write_at};
    use super::*;
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};

    #[test]
    fn adopting_picks_the_newest_file() {
        let dir = scratch("backdrop-adopt");
        let now = SystemTime::now();
        write_at(&dir.join("old"), b"x", now - Duration::from_secs(3000));
        write_at(&dir.join("new"), b"x", now - Duration::from_secs(10));
        write_at(&dir.join("older"), b"x", now - Duration::from_secs(9000));
        assert_eq!(ZoomUs::adopt_from(&dir).unwrap().path, dir.join("new"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn adopting_an_empty_or_missing_folder_says_what_to_do() {
        let dir = scratch("backdrop-empty");
        assert!(ZoomUs::adopt_from(&dir).is_err());
        let missing = dir.join("nowhere");
        let message = ZoomUs::adopt_from(&missing).unwrap_err().to_string();
        assert!(message.contains("zoom.us"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hung_painting_counts_as_read_only_once_accessed_after_modified() {
        let dir = scratch("backdrop-read");
        let stage = ZoomUs::at(dir.join("slot"));
        fs::write(&stage.path, b"user's own").unwrap();
        fs::write(dir.join(SPARE), b"painting").unwrap();
        stage.hang(&dir).unwrap();
        // Not read back here: reading it is exactly what would flip the answer.
        assert!(!stage.read_since_hung());

        let modified = fs::metadata(&stage.path).unwrap().modified().unwrap();
        OpenOptions::new()
            .write(true)
            .open(&stage.path)
            .unwrap()
            .set_times(FileTimes::new().set_accessed(modified + Duration::from_secs(5)))
            .unwrap();
        assert!(stage.read_since_hung());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hanging_refuses_when_the_slot_is_gone() {
        let dir = scratch("backdrop-gone");
        let spare = dir.join(SPARE);
        fs::write(&spare, b"painting").unwrap();
        let stage = ZoomUs::at(dir.join("deleted-in-zoom"));
        assert!(stage.hang(&dir).is_err());
        assert!(!stage.path.exists());
        assert!(spare.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
