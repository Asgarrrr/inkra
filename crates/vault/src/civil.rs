//! A Unix timestamp to a civil date, in UTC. Written by hand to avoid a
//! date-arithmetic dependency; no local time, since that needs a zone database.

use std::time::SystemTime;

/// `YYYY-MM-DD HH-MM-SS`, UTC, safe in a filename. Clamps to the epoch for an
/// earlier time, so callers never see an error they can't act on.
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

/// Howard Hinnant's `civil_from_days`. Correct for the full Gregorian leap
/// rule, including 1900 (not a leap year) and 2000 (one).
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
        assert_eq!(format_utc(at(1_709_210_096)), "2024-02-29 12-34-56");
    }

    #[test]
    fn a_century_boundary_that_is_a_leap_year() {
        // Divisible by 100 and 400, so it is a leap year despite the century.
        assert_eq!(format_utc(at(951_782_400)), "2000-02-29 00-00-00");
    }

    #[test]
    fn a_time_before_the_epoch_clamps() {
        assert_eq!(
            format_utc(SystemTime::UNIX_EPOCH - Duration::from_secs(1)),
            "1970-01-01 00-00-00"
        );
    }

    #[test]
    fn a_recent_timestamp() {
        assert_eq!(format_utc(at(1_790_001_125)), "2026-09-21 14-32-05");
    }
}
