use super::{civil_from_days, format_utc};
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

#[test]
fn a_century_boundary_that_is_not_a_leap_year() {
    assert_eq!(format_utc(at(4_107_542_399)), "2100-02-28 23-59-59");
    assert_eq!(format_utc(at(4_107_542_400)), "2100-03-01 00-00-00");
}

/// `format_utc` clamps before the epoch, so 1900 is reached through the day count alone.
#[test]
fn nineteen_hundred_is_not_a_leap_year() {
    assert_eq!(civil_from_days(-25_509), (1900, 2, 28));
    assert_eq!(civil_from_days(-25_508), (1900, 3, 1));
}
