use crate::flex::valid;

#[test]
fn valid_takes_positive_flexes_one_per_child_summing_to_their_count() {
    for (flexes, len) in [
        (&[1.0, 1.0][..], 2),
        (&[0.5, 1.5], 2),
        (&[1.0, 1.0005], 2),
        (&[], 0),
    ] {
        assert!(valid(flexes, len), "{flexes:?} for {len}");
    }
}

#[test]
fn valid_refuses_a_missing_flex_a_non_positive_one_or_a_sum_off_its_count() {
    for (flexes, len) in [
        (&[2.0][..], 2),
        (&[1.0, 1.0], 3),
        (&[0.0, 2.0], 2),
        (&[-1.0, 3.0], 2),
        (&[1.0, 1.0015], 2),
        (&[1.0, 0.998], 2),
    ] {
        assert!(!valid(flexes, len), "{flexes:?} for {len}");
    }
}
