//! `toggle_task`: a box's click (task boxes spec, D5).

use proptest::prelude::*;

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// `text` toggled at the checkbox starting at `start`, from a caret at the text's end.
fn toggled(text: &str, start: usize) -> (Buffer, EditorState) {
    let mut buffer = Buffer::new(text);
    let mut state = EditorState::new(Selection::at(buffer.len()));
    state.toggle_task(&mut buffer, at(start)..at(start + 3));
    (buffer, state)
}

#[test]
fn an_open_task_ticks_and_the_caret_lands_on_its_text() {
    let (buffer, state) = toggled("- [ ] a\nb", 2);
    assert_eq!(buffer.text(), "- [x] a\nb");
    assert_eq!(state.selection().range(), at(6)..at(6));
}

#[test]
fn a_ticked_task_clears_in_either_case() {
    for text in ["- [x] a", "- [X] a"] {
        let (buffer, state) = toggled(text, 2);
        assert_eq!(buffer.text(), "- [ ] a", "{text}");
        assert_eq!(state.cursor(), at(6), "{text}");
    }
}

#[test]
fn the_text_start_is_past_one_tab_and_no_further() {
    let (buffer, state) = toggled("> 1. [ ]\t\ta", 5);
    assert_eq!(buffer.text(), "> 1. [x]\t\ta");
    assert_eq!(state.cursor(), at(9));
}

/// A task with no text puts the caret at its line's end, terminated or not.
#[test]
fn a_task_with_no_text_puts_the_caret_at_its_line_end() {
    for text in ["- [ ]\nb", "- [ ]"] {
        let (_, state) = toggled(text, 2);
        assert_eq!(state.cursor(), at(5), "{text:?}");
    }
}

#[test]
fn bytes_that_are_no_checkbox_edit_nothing() {
    for (text, start) in [
        ("- [y] a", 2),
        ("- [ ] a", 1),
        ("- [ ] a", 3),
        ("- (x) a", 2),
    ] {
        let (mut buffer, mut state) = toggled(text, start);
        assert_eq!(buffer.text(), text);
        assert_eq!(state.cursor(), buffer.len(), "{text}");
        assert!(!state.undo(&mut buffer), "{text}: no step was recorded");
    }
}

/// One step of its own: the typing on either side, within one group interval, stays apart.
#[test]
fn a_toggle_is_one_step_that_merges_with_no_typing() {
    let mut buffer = Buffer::new("- [ ] ");
    let mut state = EditorState::new(Selection::at(buffer.len()));
    let start = Instant::now();

    state.insert(&mut buffer, start, "1");
    state.toggle_task(&mut buffer, at(2)..at(5));
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2, "2");
    assert_eq!(buffer.text(), "- [x] 21");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [x] 1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [ ] 1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [ ] ");
}

proptest! {
    /// Only the middle byte changes, two toggles undo each other, and one undo restores the text.
    #[test]
    fn a_toggle_flips_the_middle_byte_alone(
        before in "[a-z> -]{0,6}",
        middle in prop::sample::select(vec![' ', 'x', 'X']),
        after in "[ \ta-zé\n]{0,6}",
    ) {
        let text = format!("{before}[{middle}]{after}");
        let start = before.len();
        let (mut buffer, mut state) = toggled(&text, start);
        let flipped = if middle == ' ' { 'x' } else { ' ' };
        prop_assert_eq!(buffer.text(), format!("{before}[{flipped}]{after}"));
        let spaced = after.starts_with([' ', '\t']);
        prop_assert_eq!(state.cursor(), at(start + 3 + usize::from(spaced)));

        state.toggle_task(&mut buffer, at(start)..at(start + 3));
        let back = if middle == 'X' { format!("{before}[x]{after}") } else { text.clone() };
        prop_assert_eq!(buffer.text(), back);

        prop_assert!(state.undo(&mut buffer));
        prop_assert!(state.undo(&mut buffer));
        prop_assert_eq!(buffer.text(), text);
    }
}
