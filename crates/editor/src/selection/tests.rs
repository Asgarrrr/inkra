use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

#[test]
fn a_fresh_selection_is_an_empty_forward_cursor() {
    let selection = Selection::at(at(3));
    assert!(selection.is_empty());
    assert!(!selection.reversed());
    assert_eq!(selection.head(), at(3));
    assert_eq!(selection.tail(), at(3));
}

#[test]
fn extending_forward_moves_the_end_and_anchors_the_start() {
    let mut selection = Selection::at(at(1));
    selection.set_head(at(4), Goal::None);
    assert_eq!(selection.range(), at(1)..at(4));
    assert!(!selection.reversed());
    assert_eq!(selection.head(), at(4));
    assert_eq!(selection.tail(), at(1));
}

#[test]
fn extending_backward_flips_reversed_without_inverting_the_range() {
    let mut selection = Selection::at(at(4));
    selection.set_head(at(1), Goal::None);
    assert_eq!(selection.range(), at(1)..at(4));
    assert!(selection.reversed());
    assert_eq!(selection.head(), at(1));
}

#[test]
fn dragging_the_head_across_the_tail_flips_direction_once() {
    let mut selection = Selection::at(at(2));
    selection.set_head(at(5), Goal::None);
    assert!(!selection.reversed());

    selection.set_head(at(0), Goal::None);
    assert_eq!(selection.range(), at(0)..at(2));
    assert!(selection.reversed());
    assert_eq!(selection.tail(), at(2));
}

#[test]
fn collapsing_clears_the_reversed_flag() {
    let mut selection = Selection::new(at(1)..at(4), true);
    selection.collapse_to(at(4), Goal::None);
    assert!(selection.is_empty());
    assert!(!selection.reversed());
}

#[test]
#[should_panic(expected = "is inverted")]
fn an_inverted_range_is_refused() {
    let _ = Selection::new(at(4)..at(1), false);
}

#[test]
fn setting_the_head_exactly_on_the_tail_is_not_reversed() {
    let mut selection = Selection::at(at(2));
    selection.set_head(at(2), Goal::None);
    assert!(selection.is_empty());
    assert!(!selection.reversed());
}
