//! The sorted-offset lookups.

use super::{first_from, last_before};

#[test]
fn first_from_finds_the_first_key_at_or_past_its_offset() {
    let keys = [1, 3, 5];
    for (at, want) in [(0, Some(1)), (3, Some(3)), (4, Some(5)), (6, None)] {
        assert_eq!(first_from(&keys, at, |&key| key).copied(), want, "{at}");
    }
}

#[test]
fn last_before_finds_the_last_key_before_its_offset() {
    let keys = [1, 3, 5];
    for (at, want) in [(1, None), (3, Some(0)), (4, Some(1)), (6, Some(2))] {
        assert_eq!(last_before(&keys, at, |&key| key), want, "{at}");
    }
}
