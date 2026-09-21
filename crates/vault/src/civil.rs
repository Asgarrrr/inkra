//! A Unix timestamp to a civil date, in UTC.
//!
//! Twenty lines rather than a dependency, per `dependencies.md`: date
//! arithmetic is named there as the write-it-yourself case. Local time is not
//! offered, because rendering it needs the zone database — `libc::localtime_r`
//! and a new direct dependency — to save the reader an offset they can apply
//! themselves.

use std::time::SystemTime;

/// `YYYY-MM-DD HH-MM-SS`, UTC. Safe in a filename: no colon, no slash.
///
/// A time before the epoch clamps to the epoch. No file in a vault carries one,
/// and a total function keeps the caller from handling an error it could not
/// act on anyway.
pub(crate) fn format_utc(at: SystemTime) -> String {
    let seconds = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}-{:02}-{:02}",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60,
    )
}

/// Howard Hinnant's `civil_from_days`, the inverse of the days-from-civil
/// algorithm `std::chrono` uses. Shifts the era so March is month 1, which is
/// what removes the leap day from the middle of the arithmetic: it then falls
/// at the end of a year rather than inside one.
///
/// Correct for the Gregorian rule in full, including that 1900 is not a leap
/// year and 2000 is.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::format_utc;
    use std::time::{Duration, SystemTime};

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn the_epoch_itself() {
        assert_eq!(format_utc(at(0)), "1970-01-01 00-00-00");
    }

    #[test]
    fn a_leap_day() {
        // 2024-02-29T12:34:56Z
        assert_eq!(format_utc(at(1_709_210_096)), "2024-02-29 12-34-56");
    }

    #[test]
    fn a_century_boundary_that_is_a_leap_year() {
        // 2000-02-29T00:00:00Z. Divisible by 100 and by 400, so it *is* a leap
        // year — the case a three-line leap rule gets wrong by a day.
        assert_eq!(format_utc(at(951_782_400)), "2000-02-29 00-00-00");
    }

    #[test]
    fn a_time_before_the_epoch_clamps() {
        // `SystemTime` before the epoch cannot be expressed as a duration since
        // it. Nothing in a vault should carry one; clamping keeps the name
        // total rather than making the caller handle an error it cannot act on.
        assert_eq!(
            format_utc(SystemTime::UNIX_EPOCH - Duration::from_secs(1)),
            "1970-01-01 00-00-00"
        );
    }

    #[test]
    fn a_recent_timestamp() {
        // 2026-09-21T14:32:05Z
        assert_eq!(format_utc(at(1_790_001_125)), "2026-09-21 14-32-05");
    }
}
