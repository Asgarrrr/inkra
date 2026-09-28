use inkra_text::diff;

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

/// The state after `old` becomes `new`, the selection `range` set on `old` first.
fn mapped(old: &str, new: &str, range: Range<usize>, reversed: bool) -> EditorState {
    let mut state = EditorState::new(Selection::new(at(range.start)..at(range.end), reversed));
    state.map_through(&diff(0, old, new));
    state
}

#[test]
fn a_caret_inside_a_replaced_word_lands_after_the_replacement() {
    let state = mapped("a quick fox", "a slow fox", 4..4, false);
    assert_eq!(state.selection().range(), at(6)..at(6));
}

#[test]
fn a_caret_moves_with_the_text_inserted_before_it() {
    let state = mapped("one\ntwo\n", "zero\none\ntwo\n", 5..5, false);
    assert_eq!(state.cursor(), at(10));
}

#[test]
fn a_selection_does_not_grow_at_either_edge() {
    let state = mapped(
        "one\ntwo\nthree\n",
        "one\nNEW\ntwo\nNEW\nthree\n",
        4..8,
        true,
    );
    assert_eq!(state.selection().range(), at(8)..at(12));
    assert!(state.selection().reversed());
}

#[test]
fn a_selection_inside_one_hunk_collapses_to_the_mapped_start() {
    let state = mapped("a quick fox", "a slow fox", 3..5, true);
    assert_eq!(state.selection().range(), at(6)..at(6));
    assert!(!state.selection().reversed());

    let deleted = mapped("a quick fox", "a fox", 2..7, true);
    assert_eq!(deleted.selection().range(), at(2)..at(2));
    assert!(!deleted.selection().reversed());
}

#[test]
fn a_marked_range_keeps_its_text_and_one_removed_is_dropped() {
    let (old, new) = ("a bc d", "X a bc d");
    let mut state = EditorState::new(Selection::at(at(4)));
    state.set_marked_range(Some(at(2)..at(4)));
    state.map_through(&diff(0, old, new));
    assert_eq!(state.marked_range(), Some(at(4)..at(6)));

    state.map_through(&diff(0, new, "X a Q d"));
    assert_eq!(state.marked_range(), None);

    state.set_marked_range(Some(at(4)..at(7)));
    state.map_through(&diff(0, "one\ntwo\nthree", "one\nthree"));
    assert_eq!(
        state.marked_range(),
        None,
        "a deleted composition leaves no empty range"
    );
}

#[test]
fn the_goal_resets() {
    let mut state = EditorState::new(Selection::at(at(0)));
    let buffer = Buffer::new("ab");
    state.move_vertically(&buffer, at(1), false, Goal::Column(7.));
    state.map_through(&diff(0, "ab", "ab\n"));
    assert_eq!(state.selection().goal(), Goal::None);
}
