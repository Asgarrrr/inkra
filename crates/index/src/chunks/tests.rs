use std::ops::Range;

use super::split;

const WHOLE: Range<usize> = 0..10;

#[test]
fn ranges_cover_the_length_in_order() {
    assert_eq!(split(10, 3), [0..3, 3..6, 6..10]);
    assert_eq!(split(10, 1), [WHOLE]);
}

#[test]
fn zero_parts_means_one() {
    assert_eq!(split(10, 0), [WHOLE]);
}

#[test]
fn no_range_is_empty() {
    assert_eq!(split(3, 8), [0..1, 1..2, 2..3]);
    assert!(split(0, 4).is_empty());
}
