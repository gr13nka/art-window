//! Which day it is, where the user is.
//!
//! The rotation owes a picture once the date has changed, and a date is a local
//! thing: an hour that is already Tuesday in Kyiv is still Monday in Chicago.
//! Nothing else in the program needs a calendar, so this is the whole of one — one
//! number per instant, and comparison is the only operation on it.

/// When a day begins, in seconds after local midnight: five in the morning.
///
/// Not midnight, because midnight is often still last night. A laptop open at
/// half past twelve would spend the new day on a picture its owner sees before
/// bed, and the morning would open on that same "old" painting. Five o'clock is
/// past anyone's late night and before anyone's morning, so the first time the lid
/// opens after it, the picture is new.
const DAY_BEGINS: i64 = 5 * 60 * 60;

/// The local day the instant `at` — Unix seconds — falls in, numbered from the
/// epoch, each one running from [`DAY_BEGINS`] to the same hour the next morning.
///
/// Only the difference between two of these means anything. The number is not a
/// date, cannot be formatted as one, and is never shown to anyone.
pub fn local(at: u64) -> i64 {
    const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
    // Euclidean division, so an instant before 1970 floors towards the earlier day
    // rather than towards zero. A clock that wrong is nobody's real problem, but
    // truncation there would read as "same day" — the one answer that stops the
    // rotation rather than nudging it.
    (at as i64 + offset(at) - DAY_BEGINS).div_euclid(SECONDS_PER_DAY)
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

/// GLib asks the system timezone database for the interval containing this UTC
/// instant, so each side of a daylight-saving boundary gets its own real offset.
#[cfg(target_os = "linux")]
fn offset(at: u64) -> i64 {
    let zone = glib::TimeZone::local();
    offset_in(&zone, at)
}

#[cfg(target_os = "linux")]
fn offset_in(zone: &glib::TimeZone, at: u64) -> i64 {
    let interval = zone.find_interval(glib::TimeType::Universal, at as i64);
    if interval < 0 {
        0
    } else {
        zone.offset(interval) as i64
    }
}

/// Windows converts the instant itself: the same UTC time is turned into local time
/// under the zone's rules for *that* moment, and the gap between the two readings
/// is the offset.
#[cfg(windows)]
fn offset(at: u64) -> i64 {
    use windows::Win32::System::Time::{
        GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };

    let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: `zone` is a valid, writable structure. Its return value only says
    // which of standard or daylight time is in force now, which is not asked here.
    unsafe { GetDynamicTimeZoneInformation(&mut zone) };
    offset_in(&zone, at)
}

#[cfg(windows)]
fn offset_in(zone: &windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION, at: u64) -> i64 {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
    };

    // FILETIME counts 100 ns ticks from 1601; Unix time counts seconds from 1970.
    const TICKS_PER_SECOND: u64 = 10_000_000;
    const EPOCH_GAP_SECONDS: u64 = 11_644_473_600;
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);

    let ticks_at = (at + EPOCH_GAP_SECONDS) * TICKS_PER_SECOND;
    let utc = FILETIME {
        dwLowDateTime: ticks_at as u32,
        dwHighDateTime: (ticks_at >> 32) as u32,
    };
    let mut utc_parts = SYSTEMTIME::default();
    let mut local_parts = SYSTEMTIME::default();
    let mut local = FILETIME::default();
    // SAFETY: every pointer is to a live local of the type the call names.
    let converted = unsafe {
        FileTimeToSystemTime(&utc, &mut utc_parts)
            .and_then(|()| {
                SystemTimeToTzSpecificLocalTimeEx(Some(zone), &utc_parts, &mut local_parts)
            })
            .and_then(|()| SystemTimeToFileTime(&local_parts, &mut local))
    };
    // An instant the calendar cannot express has no offset worth applying, and zero
    // is the answer that leaves the day unmoved rather than the one that stops it.
    if converted.is_err() {
        return 0;
    }
    (ticks(local) as i64 - ticks_at as i64) / TICKS_PER_SECOND as i64
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::offset_in;

    #[test]
    #[allow(deprecated)]
    fn asks_for_the_offset_at_each_instant() {
        let new_york = glib::TimeZone::new(Some("America/New_York"));
        assert_eq!(offset_in(&new_york, 1_704_067_200), -5 * 60 * 60);
        assert_eq!(offset_in(&new_york, 1_719_792_000), -4 * 60 * 60);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::offset_in;
    use windows::Win32::System::Time::{
        EnumDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };

    fn new_york() -> DYNAMIC_TIME_ZONE_INFORMATION {
        for index in 0.. {
            let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
            // SAFETY: `zone` is valid and writable; running out of zones is an error.
            if unsafe { EnumDynamicTimeZoneInformation(index, &mut zone) } != 0 {
                break;
            }
            let key = String::from_utf16_lossy(&zone.TimeZoneKeyName);
            if key.trim_end_matches('\0') == "Eastern Standard Time" {
                return zone;
            }
        }
        panic!("Eastern Standard Time is not in the timezone database");
    }

    #[test]
    fn asks_for_the_offset_at_each_instant() {
        let zone = new_york();
        assert_eq!(offset_in(&zone, 1_704_067_200), -5 * 60 * 60);
        assert_eq!(offset_in(&zone, 1_719_792_000), -4 * 60 * 60);
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::{local, offset, DAY_BEGINS};

    /// Local midnight before `at`, in whatever timezone the tests run in. Picked
    /// in late September, when no zone changes its clocks, so the hours that
    /// follow are plain hours.
    fn midnight_before(at: u64) -> u64 {
        let into_day = (at as i64 + offset(at)).rem_euclid(24 * 60 * 60);
        (at as i64 - into_day) as u64
    }

    #[test]
    fn a_day_turns_over_at_five_in_the_morning_not_at_midnight() {
        let midnight = midnight_before(1_790_200_000);
        let five = midnight + DAY_BEGINS as u64;
        assert_eq!(local(midnight - 60), local(midnight + 60));
        assert_ne!(local(five - 60), local(five + 60));
    }
}
