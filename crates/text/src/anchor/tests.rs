use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

#[test]
fn an_offset_before_the_edit_does_not_move() {
    for bias in [Bias::Left, Bias::Right] {
        assert_eq!(transform(at(0), bias, at(2), 2, 3), at(0));
        assert_eq!(transform(at(1), bias, at(2), 2, 3), at(1));
    }
}

#[test]
fn an_offset_after_the_edit_shifts_by_the_length_change() {
    for bias in [Bias::Left, Bias::Right] {
        assert_eq!(transform(at(5), bias, at(1), 2, 3), at(6));
        assert_eq!(transform(at(5), bias, at(1), 2, 1), at(4));
    }
}

#[test]
fn a_pure_insertion_at_the_offset_is_decided_by_bias() {
    assert_eq!(transform(at(2), Bias::Left, at(2), 0, 3), at(2));
    assert_eq!(transform(at(2), Bias::Right, at(2), 0, 3), at(5));
}

#[test]
fn an_offset_inside_the_removed_span_collapses_to_its_bias_side() {
    for inserted_len in [1, 5] {
        assert_eq!(transform(at(2), Bias::Left, at(1), 3, inserted_len), at(1));
        assert_eq!(
            transform(at(2), Bias::Right, at(1), 3, inserted_len),
            ByteOffset::new(1 + inserted_len)
        );
    }
}

#[test]
fn the_start_of_a_non_empty_replace_is_decided_by_bias() {
    assert_eq!(transform(at(1), Bias::Left, at(1), 3, 2), at(1));
    assert_eq!(transform(at(1), Bias::Right, at(1), 3, 2), at(3));
}

#[test]
fn the_exclusive_end_of_a_replace_shifts_under_both_biases() {
    for bias in [Bias::Left, Bias::Right] {
        assert_eq!(transform(at(4), bias, at(1), 3, 2), at(3));
    }
}

#[test]
fn the_end_of_a_pure_deletion_tracks_onto_its_start() {
    assert_eq!(transform(at(1), Bias::Right, at(0), 1, 0), at(0));
    assert_eq!(transform(at(1), Bias::Left, at(0), 1, 0), at(0));
}
