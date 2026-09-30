use inkra_text::ByteOffset;

use super::*;
use crate::selection::Goal;

fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

fn caret(offset: usize, goal: Goal) -> Selection {
    let mut selection = Selection::at(at(offset));
    selection.collapse_to(at(offset), goal);
    selection
}

fn edit(buffer: &mut Buffer, range: std::ops::Range<usize>, text: &str) {
    buffer.transact(|buffer| buffer.replace(at(range.start)..at(range.end), text));
}

#[test]
fn a_parked_caret_follows_an_insert_before_it_and_at_it() {
    let mut buffer = Buffer::new("hello world");
    let parked = ParkedSelection::park(&Selection::at(at(6)), &mut buffer);
    edit(&mut buffer, 0..0, "oh ");
    edit(&mut buffer, 9..9, "big ");
    assert_eq!(parked.unpark(&mut buffer).range(), at(13)..at(13));
}

#[test]
fn a_parked_range_does_not_grow_at_either_edge() {
    let mut buffer = Buffer::new("one two three");
    let parked = ParkedSelection::park(&Selection::new(at(4)..at(7), false), &mut buffer);
    edit(&mut buffer, 7..7, "X");
    edit(&mut buffer, 4..4, "Y");
    let selection = parked.unpark(&mut buffer);
    assert_eq!(buffer.text(), "one YtwoX three");
    assert_eq!(selection.range(), at(5)..at(8));
}

#[test]
fn a_parked_range_inside_a_deletion_collapses_to_its_start() {
    let mut buffer = Buffer::new("a quick fox");
    let parked = ParkedSelection::park(&Selection::new(at(3)..at(5), true), &mut buffer);
    edit(&mut buffer, 2..8, "");
    let selection = parked.unpark(&mut buffer);
    assert_eq!(selection.range(), at(2)..at(2));
    assert!(!selection.reversed());
}

#[test]
fn a_reversed_range_keeps_its_direction() {
    let mut buffer = Buffer::new("one two three");
    let parked = ParkedSelection::park(&Selection::new(at(4)..at(7), true), &mut buffer);
    edit(&mut buffer, 0..0, "zero ");
    let selection = parked.unpark(&mut buffer);
    assert_eq!(selection.range(), at(9)..at(12));
    assert!(selection.reversed());
}

#[test]
fn an_unchanged_buffer_gives_back_the_goal() {
    let mut buffer = Buffer::new("one\ntwo\n");
    let parked = ParkedSelection::park(&caret(5, Goal::Column(12.5)), &mut buffer);
    assert_eq!(parked.unpark(&mut buffer), caret(5, Goal::Column(12.5)));
}

#[test]
fn an_edit_drops_the_goal() {
    let mut buffer = Buffer::new("one\ntwo\n");
    let parked = ParkedSelection::park(&caret(2, Goal::Column(12.5)), &mut buffer);
    edit(&mut buffer, 6..6, "X");
    let selection = parked.unpark(&mut buffer);
    assert_eq!(selection.range(), at(2)..at(2));
    assert_eq!(selection.goal(), Goal::None);
}

#[test]
fn an_undo_redo_round_trip_that_moved_an_anchor_drops_the_goal() {
    let mut buffer = Buffer::new("0123456789");
    let before = buffer.version();
    let parked = ParkedSelection::park(&caret(5, Goal::Column(40.0)), &mut buffer);
    edit(&mut buffer, 3..8, "");
    assert!(buffer.undo().is_some());
    assert_eq!(buffer.version(), before);
    let selection = parked.unpark(&mut buffer);
    assert_eq!(selection.range(), at(8)..at(8));
    assert_eq!(selection.goal(), Goal::None);
}

#[test]
#[should_panic(expected = "not live")]
fn unpark_forgets_its_anchors() {
    let mut buffer = Buffer::new("hello");
    let parked = ParkedSelection::park(&Selection::new(at(1)..at(3), false), &mut buffer);
    let end = parked.end;
    let _ = parked.unpark(&mut buffer);
    let _ = buffer.resolve(end);
}
