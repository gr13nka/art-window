//! Which day it is, where the user is.
//!
//! The rotation owes a picture once the date has changed, and a date is a local
//! thing: an hour that is already Tuesday in Kyiv is still Monday in Chicago.
//! Nothing else in the program needs a calendar, so this is the whole of one — one
//! number per instant, and comparison is the only operation on it.

/// The local calendar day the instant `at` — Unix seconds — falls on, numbered from
/// the epoch.
///
/// Only the difference between two of these means anything. The number is not a
/// date, cannot be formatted as one, and is never shown to anyone.
pub fn local(at: u64) -> i64 {
    const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
    // Euclidean division, so an instant before 1970 floors towards the earlier day
    // rather than towards zero. A clock that wrong is nobody's real problem, but
    // truncation there would read as "same day" — the one answer that stops the
    // rotation rather than nudging it.
    (at as i64 + offset(at)).div_euclid(SECONDS_PER_DAY)
}

/// Seconds east of UTC where the user is, as of `at`.
///
/// Asked for the instant in question and not for now, so that the two days being
/// compared are each measured under the offset actually in force. Asking once for
/// both would misread the hour either side of a daylight-saving change as a day
/// that has or has not turned over.
#[cfg(target_os = "macos")]
fn offset(at: u64) -> i64 {
    use objc2_foundation::{NSDate, NSTimeZone};

    let when = NSDate::dateWithTimeIntervalSince1970(at as f64);
    NSTimeZone::localTimeZone().secondsFromGMTForDate(&when) as i64
}

/// UTC, until there is a backend that knows better.
///
/// This joins the rest of the platform work that is planned and not built. On a
/// machine far from Greenwich the picture changes at the wrong hour of the local
/// evening or morning — wrong, but not stuck, which is the failure worth avoiding.
#[cfg(not(target_os = "macos"))]
fn offset(_at: u64) -> i64 {
    0
}
