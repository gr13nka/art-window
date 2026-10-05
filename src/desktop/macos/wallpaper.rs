//! macOS desktop-image backend.
//!
//! Two mechanisms, because one is not enough.
//!
//! `NSWorkspace` is the supported API, and it is what makes the change appear
//! instantly — but it only ever touches the Space that happens to be active. macOS
//! keeps a separate wallpaper for every Mission Control Space on every display, and
//! offers no public way to reach the others. A machine with twenty Spaces would show
//! the new picture on exactly one of them.
//!
//! So the rest are written straight into the Dock's own database. That file is
//! private to Apple and its shape could change, so a failure there is reported and
//! stepped over rather than treated as fatal: the supported path still puts the
//! picture on the Space the user is looking at. It is reported to the *caller* as
//! well as to the log, because at login that failure is the ordinary case and
//! somebody has to come back and ask again — see [`Pinned`].
//!
//! And the writing alone changes nothing anybody can see. The Dock keeps the whole
//! store in memory and re-reads it only when it starts, so publishing a change to
//! the other Spaces means restarting the Dock — which blanks every desktop, for
//! half a minute on a tired machine. That is far too much to spend at the moment a
//! picture changes, when the Space being looked at is already correct and the ones
//! waiting are by definition not being looked at. So [`pin`] writes and says
//! [`Pinned::AfterRedraw`]; [`catch_up`] is the restart, for a moment the desktop
//! was going to be redrawn anyway.

use crate::desktop::Pinned;
use crate::journal;
use crate::placement::{Hang, Mode};
use anyhow::{anyhow, Context, Result};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSColor, NSImageScaling, NSScreen, NSWorkspace, NSWorkspaceDesktopImageAllowClippingKey,
    NSWorkspaceDesktopImageFillColorKey, NSWorkspaceDesktopImageOptionKey,
    NSWorkspaceDesktopImageScalingKey,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Placement code the Dock stores for "scale to fit, letterbox the remainder".
/// The same meaning as `NSImageScaling::ScaleProportionallyUpOrDown` with clipping
/// off, in the Dock's own numbering.
const DOCK_PLACEMENT_FIT: i64 = 5;
/// Placement code for "scale to fill the screen, cropping the overflow".
///
/// UNVERIFIED. The development machine's store only ever holds `5`; the other
/// codes are the commonly reported ones for `preferences.key = 2` and were not
/// confirmed against a real change of setting, because doing so would change the
/// user's desktop. If Zoom lands as something else on the other Spaces, this is
/// the first place to look.
const DOCK_PLACEMENT_FILL: i64 = 1;
/// Placement code for "stretch to the screen's shape". UNVERIFIED, as above.
const DOCK_PLACEMENT_STRETCH: i64 = 3;

fn dock_placement(mode: Mode) -> i64 {
    match mode {
        Mode::Fit => DOCK_PLACEMENT_FIT,
        Mode::Fill => DOCK_PLACEMENT_FILL,
        Mode::Stretch => DOCK_PLACEMENT_STRETCH,
    }
}

/// A colour channel as the Dock stores it and AppKit takes it: a float in 0..1.
fn unit(channel: u8) -> f64 {
    f64::from(channel) / 255.0
}

/// The main display's size in device pixels, or `None` off the main thread.
pub fn primary_screen() -> Option<(u32, u32)> {
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let size = screen.frame().size;
    let scale = screen.backingScaleFactor();
    Some((
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    ))
}

/// How long to wait for the Dock to let go of its own database.
///
/// The Dock writes this file too, and by default neither side waits for the other:
/// a moment's overlap makes both fail, which is exactly what happens while a
/// session is coming up. Waiting costs nothing worth counting — a picture only
/// goes up on an event loop that has just finished waiting minutes for a download
/// — and it is the difference between a login-time write landing and not.
const DOCK_BUSY_WAIT: Duration = Duration::from_millis(500);

/// The store first, AppKit second, and the order is the point. The Dock answers
/// the AppKit call by writing the same store a moment later, and it does not wait
/// for a lock: finding this program's transaction open, it gives the file up as
/// corrupt and starts an empty one, which the next restart then publishes to every
/// Space. So the transaction is committed and its connection closed before the
/// Dock is asked for anything.
pub fn pin(hang: &Hang) -> Result<Pinned> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        anyhow!(
            "wallpaper must be set from the main thread: AppKit enumerates displays nowhere else"
        )
    })?;

    let began = Instant::now();
    journal::note!(
        "pin",
        "path {}, mode {:?}, colour {:?}",
        hang.path.display(),
        hang.mode,
        hang.colour
    );
    let spread = spread_to_every_space(hang);
    // The connection is closed by now, which is the only moment the store may be
    // looked at again.
    store_health();
    if let Err(e) = set_active_space(hang, mtm) {
        journal::fault("pin", &e);
        return Err(e);
    }

    let pinned = match spread {
        Ok(true) => Pinned::AfterRedraw,
        Ok(false) => Pinned::Everywhere,
        Err(e) => {
            journal::note!(
                "pin",
                "only the active Space was updated; the Dock's wallpaper store was not usable ({e:#})"
            );
            Pinned::InPart
        }
    };
    journal::note!(
        "pin",
        "answered {pinned:?} in {} ms",
        began.elapsed().as_millis()
    );
    Ok(pinned)
}

/// The supported route: AppKit, for whichever Space is in front right now.
fn set_active_space(hang: &Hang, mtm: MainThreadMarker) -> Result<()> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&hang.path.to_string_lossy()));
    let options = desktop_image_options(hang);
    let workspace = NSWorkspace::sharedWorkspace();

    let screens = NSScreen::screens(mtm);
    if screens.is_empty() {
        return Err(anyhow!("no displays attached"));
    }
    journal::note!("pin", "asking AppKit for {} screens", screens.len());

    for screen in screens.iter() {
        unsafe {
            workspace
                .setDesktopImageURL_forScreen_options_error(&url, &screen, &options)
                .map_err(|e| anyhow!("macOS refused the wallpaper: {e}"))?;
        }
    }
    Ok(())
}

/// The options for a [`Hang`].
///
/// Fit-to-screen is a pair of settings rather than one: scale proportionally, and
/// forbid clipping. AppKit's header is explicit that it is `allowClipping = NO`
/// which "will make the image fully visible, but there may be empty space on the
/// sides or top and bottom" — that empty space is what the fill colour paints.
/// Fill is the same scaling with clipping allowed; stretch scales each axis on its
/// own.
fn desktop_image_options(
    hang: &Hang,
) -> Retained<NSDictionary<NSWorkspaceDesktopImageOptionKey, AnyObject>> {
    let (scaling, allow_clipping) = match hang.mode {
        Mode::Fit => (NSImageScaling::ScaleProportionallyUpOrDown, false),
        Mode::Fill => (NSImageScaling::ScaleProportionallyUpOrDown, true),
        Mode::Stretch => (NSImageScaling::ScaleAxesIndependently, false),
    };
    let scaling = NSNumber::new_usize(scaling.0);
    let clipping = NSNumber::new_bool(allow_clipping);

    // Built as calibrated RGB rather than converting a colour from another space:
    // the API accepts "colors that use or can be converted to use
    // NSCalibratedRGBColorSpace", and supplying RGB directly does not lean on that
    // conversion (`NSColor::black` lives in the calibrated *white* space).
    let [r, g, b] = hang.colour.map(unit);
    let black = NSColor::colorWithCalibratedRed_green_blue_alpha(r, g, b, 1.0);

    let keys: [&NSWorkspaceDesktopImageOptionKey; 3] = unsafe {
        [
            NSWorkspaceDesktopImageScalingKey,
            NSWorkspaceDesktopImageAllowClippingKey,
            NSWorkspaceDesktopImageFillColorKey,
        ]
    };
    let values: [&AnyObject; 3] = [&scaling, &clipping, &black];

    NSDictionary::from_slices(&keys, &values)
}

fn dock_store() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/Dock/desktoppicture.db"))
}

/// The Dock records paths with a leading `~`, and only matches its own form.
fn abbreviate(path: &Path) -> String {
    match std::env::var("HOME")
        .ok()
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Points every Space on every display at `path`.
///
/// Returns whether anything actually changed, so the caller can avoid restarting
/// the Dock when there was nothing to show it.
fn spread_to_every_space(hang: &Hang) -> Result<bool> {
    let stored = abbreviate(&hang.path);
    let placement = dock_placement(hang.mode);
    let [red, green, blue] = hang.colour.map(unit);
    let mut db = rusqlite::Connection::open(dock_store()?)?;
    db.busy_timeout(DOCK_BUSY_WAIT)?;

    let slots: i64 = db.query_row("SELECT count(*) FROM pictures", [], |r| r.get(0))?;
    // A store with no slots is one macOS has just started over, and it is the one
    // case where the active Space really is all there is to write: the Dock fills
    // `pictures` in again as Spaces are visited. Nothing changed, and nothing is
    // owed — an unwritable store would be asked again, and this one would answer
    // the same way forever.
    if slots == 0 {
        journal::note!("pin", "store has no slots, nothing to write");
        return Ok(false);
    }

    // A slot is up to date only when picture, placement and colour all match: the
    // same path can be hung as fit one day and as zoom the next.
    let already: i64 = db.query_row(
        "SELECT count(*) FROM pictures pic WHERE 5 = (
             SELECT count(*) FROM preferences p JOIN data d ON d.rowid = p.data_id
             WHERE p.picture_id = pic.rowid AND (
                 (p.key = 1 AND d.value = ?1 AND typeof(d.value) = 'text') OR
                 (p.key = 2 AND d.value = ?2 AND typeof(d.value) = 'integer') OR
                 (p.key = 3 AND d.value = ?3 AND typeof(d.value) = 'real') OR
                 (p.key = 4 AND d.value = ?4 AND typeof(d.value) = 'real') OR
                 (p.key = 5 AND d.value = ?5 AND typeof(d.value) = 'real')))",
        (&stored, placement, red, green, blue),
        |r| r.get(0),
    )?;
    if already == slots {
        journal::note!(
            "pin",
            "store slots {slots}, already current {already}, written 0"
        );
        return Ok(false);
    }

    let tx = db.transaction()?;
    {
        // Deleted first, and deliberately: a trigger on this table prunes `data`
        // rows that no longer have a referrer, so anything inserted beforehand
        // could be swept away before it is ever pointed at.
        tx.execute("DELETE FROM preferences WHERE key IN (1,2,3,4,5)", [])?;

        let path_id = intern_text(&tx, &stored)?;
        let placement_id = intern_int(&tx, placement)?;
        let channel_ids = [
            intern_real(&tx, red)?,
            intern_real(&tx, green)?,
            intern_real(&tx, blue)?,
        ];

        let picture_ids: Vec<i64> = tx
            .prepare("SELECT rowid FROM pictures")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        let mut insert =
            tx.prepare("INSERT INTO preferences(key, data_id, picture_id) VALUES (?1, ?2, ?3)")?;
        for picture_id in picture_ids {
            insert.execute((1, path_id, picture_id))?; // which image
            insert.execute((2, placement_id, picture_id))?; // how it is placed
            for (key, data_id) in (3..=5).zip(channel_ids) {
                // the margin colour, one row per channel
                insert.execute((key, data_id, picture_id))?;
            }
        }
    }
    tx.commit()?;
    journal::note!(
        "pin",
        "store slots {slots}, already current {already}, written {}",
        slots - already
    );
    Ok(true)
}

/// Writes one `dock` line on how the Dock's store looks, for reading a fault off
/// afterwards. Read-only, and it never fails its caller: a store that cannot be
/// read is itself the fact worth recording.
fn store_health() {
    let read = || -> Result<String> {
        let store = dock_store()?;
        let db = rusqlite::Connection::open_with_flags(
            &store,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(DOCK_BUSY_WAIT)?;
        let pictures: i64 = db.query_row("SELECT count(*) FROM pictures", [], |r| r.get(0))?;
        let preferences: i64 =
            db.query_row("SELECT count(*) FROM preferences", [], |r| r.get(0))?;
        let corrupt = store.with_file_name("desktoppicture.db.corrupt");
        let corrupt = match std::fs::metadata(&corrupt).and_then(|m| m.modified()) {
            Ok(at) => format!(
                "present, modified {}",
                at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
            ),
            Err(_) => "absent".to_string(),
        };
        Ok(format!(
            "pictures {pictures}, preferences {preferences}, corrupt copy {corrupt}"
        ))
    };
    match read() {
        Ok(line) => journal::note!("dock", "store {line}"),
        Err(e) => journal::note!("dock", "store unreadable: {e:#}"),
    }
}

/// `data` is a shared pool of values; rows are matched on type as well as content,
/// so the integer `5` and the string `"5"` stay distinct.
macro_rules! intern {
    ($name:ident, $ty:ty) => {
        fn $name(tx: &rusqlite::Transaction, value: $ty) -> rusqlite::Result<i64> {
            let existing = tx
                .query_row(
                    "SELECT rowid FROM data WHERE value = ?1 AND typeof(value) = typeof(?1)",
                    (&value,),
                    |r| r.get(0),
                )
                .ok();
            match existing {
                Some(id) => Ok(id),
                None => {
                    tx.execute("INSERT INTO data(value) VALUES (?1)", (&value,))?;
                    Ok(tx.last_insert_rowid())
                }
            }
        }
    };
}
intern!(intern_text, &str);
intern!(intern_int, i64);
intern!(intern_real, f64);

/// Makes the Dock re-read the store, which is the only way a write into it becomes
/// something anybody can see.
///
/// Restarting is the whole mechanism, and the reason this is not done where the
/// writing is. The Dock relaunches on its own and closes nothing, but every desktop
/// is blank until it has finished coming back — measured at around half a minute on
/// the development machine, which is a long time to look at nothing having asked
/// for a painting.
pub fn catch_up(reason: &str) {
    restart_dock(reason);
}

/// Wall-clock seconds of this program's last Dock restart; 0 for none yet.
static LAST_RESTART: AtomicU64 = AtomicU64::new(0);

fn restart_dock(reason: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let before = LAST_RESTART.swap(now, Ordering::Relaxed);
    let since = if before == 0 {
        "first restart".to_string()
    } else {
        format!("{} s since the last", now.saturating_sub(before))
    };
    let status = std::process::Command::new("/usr/bin/killall")
        .arg("Dock")
        .status();
    match status {
        Ok(status) => journal::note!("dock", "restart for {reason}, killall {status}, {since}"),
        Err(e) => journal::note!(
            "dock",
            "restart for {reason}, killall not run: {e}, {since}"
        ),
    }
}
