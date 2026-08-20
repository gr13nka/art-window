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
//! stepped over rather than treated as fatal: the supported path has already put the
//! picture on the Space the user is looking at.

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

/// Placement code the Dock stores for "scale to fit, letterbox the remainder".
/// The same meaning as `NSImageScaling::ScaleProportionallyUpOrDown` with clipping
/// off, in the Dock's own numbering.
const DOCK_PLACEMENT_FIT: i64 = 5;

pub fn pin(path: &Path) -> Result<()> {
    set_active_space(path)?;

    match spread_to_every_space(path) {
        Ok(true) => restart_dock(),
        Ok(false) => {}
        Err(e) => eprintln!(
            "note: only the active Space was updated; the Dock's wallpaper store was \
             not usable ({e})"
        ),
    }
    Ok(())
}

/// The supported route: AppKit, for whichever Space is in front right now.
fn set_active_space(path: &Path) -> Result<()> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        anyhow!(
            "wallpaper must be set from the main thread: AppKit enumerates displays nowhere else"
        )
    })?;

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let options = fit_letterboxed_in_black();
    let workspace = NSWorkspace::sharedWorkspace();

    let screens = NSScreen::screens(mtm);
    if screens.is_empty() {
        return Err(anyhow!("no displays attached"));
    }

    for screen in screens.iter() {
        unsafe {
            workspace
                .setDesktopImageURL_forScreen_options_error(&url, &screen, &options)
                .map_err(|e| anyhow!("macOS refused the wallpaper: {e}"))?;
        }
    }
    Ok(())
}

/// The options that mean "show the whole picture, paint the leftovers black".
///
/// Fit-to-screen is a pair of settings rather than one: scale proportionally, and
/// forbid clipping. AppKit's header is explicit that it is `allowClipping = NO`
/// which "will make the image fully visible, but there may be empty space on the
/// sides or top and bottom" — that empty space is what the fill colour paints.
fn fit_letterboxed_in_black() -> Retained<NSDictionary<NSWorkspaceDesktopImageOptionKey, AnyObject>>
{
    let scaling = NSNumber::new_usize(NSImageScaling::ScaleProportionallyUpOrDown.0);
    let clipping = NSNumber::new_bool(false);

    // Built as calibrated RGB rather than reusing `NSColor::black`, which lives in
    // the calibrated *white* space. The API accepts "colors that use or can be
    // converted to use NSCalibratedRGBColorSpace"; supplying RGB directly does not
    // lean on that conversion.
    let black = NSColor::colorWithCalibratedRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);

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
fn spread_to_every_space(path: &Path) -> Result<bool> {
    let stored = abbreviate(path);
    let mut db = rusqlite::Connection::open(dock_store()?)?;

    let slots: i64 = db.query_row("SELECT count(*) FROM pictures", [], |r| r.get(0))?;
    if slots == 0 {
        return Ok(false);
    }

    let already: i64 = db.query_row(
        "SELECT count(DISTINCT p.picture_id) FROM preferences p
         JOIN data d ON d.rowid = p.data_id
         WHERE p.key = 1 AND d.value = ?1",
        [&stored],
        |r| r.get(0),
    )?;
    if already == slots {
        return Ok(false);
    }

    let tx = db.transaction()?;
    {
        // Deleted first, and deliberately: a trigger on this table prunes `data`
        // rows that no longer have a referrer, so anything inserted beforehand
        // could be swept away before it is ever pointed at.
        tx.execute("DELETE FROM preferences WHERE key IN (1,2,3,4,5)", [])?;

        let path_id = intern_text(&tx, &stored)?;
        let fit_id = intern_int(&tx, DOCK_PLACEMENT_FIT)?;
        let black_id = intern_real(&tx, 0.0)?;

        let picture_ids: Vec<i64> = tx
            .prepare("SELECT rowid FROM pictures")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        let mut insert =
            tx.prepare("INSERT INTO preferences(key, data_id, picture_id) VALUES (?1, ?2, ?3)")?;
        for picture_id in picture_ids {
            insert.execute((1, path_id, picture_id))?; // which image
            insert.execute((2, fit_id, picture_id))?; // how it is placed
            for channel in 3..=5 {
                // the letterbox colour, one row per channel
                insert.execute((channel, black_id, picture_id))?;
            }
        }
    }
    tx.commit()?;
    Ok(true)
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

/// The Dock holds the wallpaper in memory, so a written change is invisible until
/// it reloads. It relaunches immediately and closes nothing.
fn restart_dock() {
    let _ = std::process::Command::new("/usr/bin/killall")
        .arg("Dock")
        .status();
}
