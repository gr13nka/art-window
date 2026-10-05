//! What the program did, and when, in one file a person can read afterwards.
//!
//! A resident program fails where nobody is watching: at five in the morning, on
//! waking, on a Space out of sight. By the time anyone notices, the only question
//! worth asking is what happened and in what order, and the answer has to have
//! been written down already. So every decision and every outcome goes here as it
//! is made — the successes too, because a fault is usually found by the ordinary
//! line that came just before it.
//!
//! One line per fact: local time to the millisecond, the topic in brackets, then
//! the fact. Local time, and with its offset, so a line can be laid beside the
//! operating system's own logs without arithmetic.
//!
//! Nothing here can fail its caller. A journal that cannot be opened or written
//! falls back to stderr, which is also where lines go before [`open`] is called —
//! the command-line modes, and the tests.

use crate::day;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the file is cut back when the program next starts.
const TOO_LONG: u64 = 1024 * 1024;
/// How much of the end survives the cut: weeks of ordinary days.
const KEPT: u64 = 256 * 1024;

static FILE: OnceLock<Mutex<File>> = OnceLock::new();

/// Starts keeping the journal at `path`, and records a panic there if one comes.
///
/// Called once, before anything worth recording. The file is appended to, never
/// replaced: on macOS it is the same file the launch agent points stderr at, and
/// two writers appending to one file is the only arrangement both survive.
pub fn open(path: &Path) {
    match begin(path) {
        Ok(file) => {
            let _ = FILE.set(Mutex::new(file));
        }
        Err(error) => eprintln!(
            "art-window: no journal at {}: {error}; using stderr",
            path.display()
        ),
    }

    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        // The release build aborts on a panic and carries no symbols, so the
        // message and where it was raised are all there is to keep.
        write("panic", format_args!("{panic}"));
        earlier(panic);
    }));
}

fn begin(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A failed cut is no reason to go without a journal.
    let _ = cut_back(path);
    OpenOptions::new().create(true).append(true).open(path)
}

/// Keeps only the end of a file that has grown past [`TOO_LONG`].
///
/// Rewritten in place rather than renamed aside, so that anyone else holding the
/// file open for appending — launchd, for stderr — is still writing to this one.
fn cut_back(path: &Path) -> std::io::Result<()> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if file.metadata()?.len() <= TOO_LONG {
        return Ok(());
    }

    file.seek(SeekFrom::End(-(KEPT as i64)))?;
    let mut tail = Vec::with_capacity(KEPT as usize);
    file.read_to_end(&mut tail)?;
    // From the first whole line, so the file never opens on half of one.
    let whole = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);

    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&tail[whole..])
}

/// Records one fact under `topic`. Reached through [`note!`].
pub fn write(topic: &str, fact: fmt::Arguments) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let line = format!(
        "{} [{topic}] {fact}\n",
        stamp(secs, now.subsec_millis(), day::offset(secs))
    );

    // One `write` per line, so two threads' lines never interleave. A poisoned
    // lock still guards a perfectly good file.
    let written = FILE.get().is_some_and(|file| {
        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        file.write_all(line.as_bytes()).is_ok()
    });
    if !written {
        eprint!("{line}");
    }
}

/// Records one fact: `journal::note!("pin", "{slots} slots written")`.
macro_rules! note {
    ($topic:expr, $($fact:tt)+) => {
        $crate::journal::write($topic, format_args!($($fact)+))
    };
}
pub(crate) use note;

/// Records something that went wrong, with every cause behind it.
pub fn fault(topic: &str, error: &anyhow::Error) {
    write(topic, format_args!("FAILED: {error:#}"));
}

/// The instant `secs` + `millis`, as read on a clock `offset` seconds east of UTC:
/// `2026-10-04 15:48:29.272 +0200`.
fn stamp(secs: u64, millis: u32, offset: i64) -> String {
    const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
    let local = secs as i64 + offset;
    let (year, month, day) = civil(local.div_euclid(SECONDS_PER_DAY));
    let of_day = local.rem_euclid(SECONDS_PER_DAY);
    let sign = if offset < 0 { '-' } else { '+' };
    let east = offset.abs();
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{millis:03} {sign}{:02}{:02}",
        of_day / 3600,
        of_day / 60 % 60,
        of_day % 60,
        east / 3600,
        east / 60 % 60,
    )
}

/// The calendar date `days` after 1 January 1970, as year, month and day.
///
/// The one place this program turns a number into a date, and only to print it:
/// nothing is ever decided by the result — see [`crate::day`] for what is. The
/// arithmetic counts from 1 March of a 400-year cycle, which puts the leap day at
/// the end of the year where it disturbs nothing.
fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_read_off_a_day_number() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(11_017), (2000, 3, 1));
        assert_eq!(civil(20_730), (2026, 10, 4));
    }

    #[test]
    fn a_line_is_stamped_in_local_time_with_its_offset() {
        // 2026-10-04 13:48:29 UTC, read two hours east of it.
        assert_eq!(
            stamp(1_791_121_709, 272, 2 * 3600),
            "2026-10-04 15:48:29.272 +0200"
        );
        // West of UTC, across midnight and by a half hour.
        assert_eq!(
            stamp(1_791_121_709 - 13 * 3600, 5, -(3 * 3600 + 1800)),
            "2026-10-03 21:18:29.005 -0330"
        );
    }

    #[test]
    fn a_file_grown_too_long_keeps_whole_lines_from_its_end() {
        let path = std::env::temp_dir().join(format!("art-window-journal-{}", std::process::id()));
        let line = "0123456789 a line of the journal\n";
        let lines = TOO_LONG as usize / line.len() + 10;
        std::fs::write(&path, line.repeat(lines)).unwrap();

        cut_back(&path).unwrap();
        let kept = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(kept.len() as u64 <= KEPT);
        assert!(kept.len() as u64 > KEPT - line.len() as u64);
        assert!(kept.starts_with(line), "opens on a whole line");
        assert!(kept.ends_with(line));
    }

    #[test]
    fn a_short_file_is_left_alone() {
        let path = std::env::temp_dir().join(format!("art-window-short-{}", std::process::id()));
        std::fs::write(&path, "one line\n").unwrap();
        cut_back(&path).unwrap();
        let kept = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(kept, "one line\n");
    }
}
