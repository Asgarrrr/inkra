//! A host's snap: no caret rests in a run it forbids, and a step over the run takes it whole.

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// Forbids 4..6 of `- a\n  b`: the end of the line before is its start, the text its end.
fn snap(offset: ByteOffset, bias: Bias) -> ByteOffset {
    if !(4..6).contains(&offset.get()) {
        return offset;
    }
    match bias {
        Bias::Left => at(3),
        Bias::Right => at(6),
    }
}

fn editor(caret: usize) -> (Buffer, EditorState) {
    (
        Buffer::new("- a\n  b"),
        EditorState::new(Selection::at(at(caret))),
    )
}

#[test]
fn a_step_left_lands_at_the_runs_start_and_any_other_at_its_end() {
    for (from, movement, extend, range, reversed) in [
        (6, LogicalMovement::Left, false, 3..3, false),
        (6, LogicalMovement::Left, true, 3..6, true),
        (3, LogicalMovement::Right, false, 6..6, false),
        (3, LogicalMovement::Right, true, 3..6, false),
        (7, LogicalMovement::LineStart, false, 6..6, false),
    ] {
        let (buffer, mut state) = editor(from);
        state.move_(&buffer, movement, extend, snap);
        let selection = state.selection();
        assert_eq!(
            selection.range(),
            at(range.start)..at(range.end),
            "{movement:?}"
        );
        assert_eq!(selection.reversed(), reversed, "{movement:?}");
    }
}

#[test]
fn backspace_and_delete_take_the_run_whole() {
    let (mut buffer, mut state) = editor(6);
    state.backspace(&mut buffer, Instant::now(), snap);
    assert_eq!((buffer.text(), state.cursor()), ("- ab", at(3)));

    let (mut buffer, mut state) = editor(3);
    state.delete(&mut buffer, Instant::now(), snap);
    assert_eq!((buffer.text(), state.cursor()), ("- ab", at(3)));
}

#[test]
fn a_placed_caret_lands_at_the_runs_end() {
    let (buffer, mut state) = editor(0);
    state.move_to(&buffer, at(5), false, snap);
    assert_eq!(state.cursor(), at(6));
    state.move_vertically(&buffer, at(4), false, Goal::Column(2.), snap);
    assert_eq!(state.cursor(), at(6));
    assert_eq!(state.selection().goal(), Goal::Column(2.));
}

#[test]
fn a_snap_moves_a_caret_to_the_runs_end_and_widens_a_range_over_it() {
    let (buffer, mut state) = editor(0);
    state.move_to(&buffer, at(5), false, no_snap);
    state.snap(snap);
    assert_eq!(state.selection(), &Selection::at(at(6)));

    state.move_to(&buffer, at(5), false, no_snap);
    state.move_to(&buffer, at(4), true, no_snap);
    state.snap(snap);
    assert_eq!(state.selection(), &Selection::new(at(3)..at(6), true));
}

#[test]
fn a_snap_outside_the_run_keeps_the_goal() {
    let (buffer, mut state) = editor(0);
    state.move_vertically(&buffer, at(7), false, Goal::Column(2.), no_snap);
    state.snap(snap);
    assert_eq!(state.selection().goal(), Goal::Column(2.));
}
