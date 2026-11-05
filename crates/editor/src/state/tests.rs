use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

fn editor(text: &str) -> (Buffer, EditorState) {
    let buffer = Buffer::new(text);
    let state = EditorState::new(Selection::at(buffer.len()));
    (buffer, state)
}

#[test]
fn backspace_deletes_a_grapheme_not_a_char() {
    let (mut buffer, mut state) = editor("xe\u{301}");
    state.backspace(&mut buffer, Instant::now());
    assert_eq!(buffer.text(), "x");
}

#[test]
fn arrows_step_over_a_zwj_emoji_as_one_unit() {
    let (buffer, mut state) = editor("👨\u{200d}👩\u{200d}👧");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    state.move_(&buffer, LogicalMovement::Right, false);
    assert_eq!(state.cursor(), buffer.len());
    state.move_(&buffer, LogicalMovement::Left, false);
    assert_eq!(state.cursor(), at(0));
}

#[test]
fn shift_arrows_extend_and_typing_replaces_the_selection() {
    let (mut buffer, mut state) = editor("abc");
    state.move_(&buffer, LogicalMovement::Left, true);
    state.move_(&buffer, LogicalMovement::Left, true);
    assert_eq!(state.selection().range(), at(1)..at(3));
    assert!(state.selection().reversed());

    let range = state.selection().range();
    state.edit(&mut buffer, Instant::now(), range, "X");
    assert_eq!(buffer.text(), "aX");
}

#[test]
fn a_reversed_selection_collapses_to_its_near_end() {
    let (buffer, mut state) = editor("abc");
    state.move_(&buffer, LogicalMovement::Left, true);
    state.move_(&buffer, LogicalMovement::Left, false);
    assert_eq!(state.cursor(), at(2));
}

#[test]
fn a_forward_selection_collapses_to_its_far_end() {
    let (buffer, mut state) = editor("abc");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    state.move_(&buffer, LogicalMovement::Right, true);
    state.move_(&buffer, LogicalMovement::Right, false);
    assert_eq!(state.cursor(), at(1));
}

#[test]
fn replacing_a_selection_is_a_single_undo_step() {
    let (mut buffer, mut state) = editor("abc");
    state.select_all(&buffer);
    let range = state.selection().range();
    state.edit(&mut buffer, Instant::now(), range, "Z");
    assert_eq!(buffer.text(), "Z");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "abc");
    assert_eq!(state.cursor(), at(3));
}

#[test]
fn undo_places_the_cursor_on_the_restored_text() {
    let (mut buffer, mut state) = editor("abc");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    state.delete(&mut buffer, Instant::now());
    assert_eq!(buffer.text(), "bc");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "abc");
    assert_eq!(state.cursor(), at(1));

    assert!(state.redo(&mut buffer));
    assert_eq!(buffer.text(), "bc");
}

#[test]
fn undo_with_nothing_to_undo_reports_no_change() {
    let (mut buffer, mut state) = editor("abc");
    assert!(!state.undo(&mut buffer));
    assert!(!state.redo(&mut buffer));
}

#[test]
fn a_typing_run_undoes_in_one_step() {
    let (mut buffer, mut state) = editor("");
    let start = Instant::now();
    for (index, ch) in "hello".chars().enumerate() {
        let now = start + inkra_text::GROUP_INTERVAL / 2 * u32::try_from(index).unwrap();
        let range = state.selection().range();
        state.edit(&mut buffer, now, range, &ch.to_string());
    }
    assert_eq!(buffer.text(), "hello");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "");
}

#[test]
fn deleting_at_the_end_of_the_buffer_does_nothing() {
    let (mut buffer, mut state) = editor("ab");
    state.delete(&mut buffer, Instant::now());
    assert_eq!(buffer.text(), "ab");
    assert!(!state.undo(&mut buffer));
}

#[test]
fn backspace_at_the_start_of_the_buffer_does_nothing() {
    let (mut buffer, mut state) = editor("ab");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    state.backspace(&mut buffer, Instant::now());
    assert_eq!(buffer.text(), "ab");
    assert!(!state.undo(&mut buffer));
}

#[test]
fn an_edit_drops_the_composing_region() {
    let (mut buffer, mut state) = editor("ab");
    state.set_marked_range(Some(at(0)..at(1)));
    state.edit(&mut buffer, Instant::now(), at(0)..at(0), "X");
    assert_eq!(state.marked_range(), None);
}

#[test]
fn undo_drops_the_composing_region() {
    let (mut buffer, mut state) = editor("ab");
    state.edit(&mut buffer, Instant::now(), at(2)..at(2), "c");
    state.set_marked_range(Some(at(0)..at(1)));
    assert!(state.undo(&mut buffer));
    assert_eq!(state.marked_range(), None);
}

#[test]
fn moving_past_either_end_saturates() {
    let (buffer, mut state) = editor("ab");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    state.move_(&buffer, LogicalMovement::Left, false);
    assert_eq!(state.cursor(), at(0));

    state.move_(&buffer, LogicalMovement::DocEnd, false);
    state.move_(&buffer, LogicalMovement::Right, false);
    assert_eq!(state.cursor(), buffer.len());
}

#[test]
fn home_and_end_go_to_the_line_ends_not_the_document_ends() {
    let (buffer, mut state) = editor("aaa\nbbb\nccc");
    state.move_to(&buffer, at(5), false);

    state.move_(&buffer, LogicalMovement::LineStart, false);
    assert_eq!(state.cursor(), at(4));
    state.move_(&buffer, LogicalMovement::LineEnd, false);
    assert_eq!(state.cursor(), at(7));
}

#[test]
fn the_document_ends_are_still_reachable() {
    let (buffer, mut state) = editor("aaa\nbbb");
    state.move_(&buffer, LogicalMovement::DocStart, false);
    assert_eq!(state.cursor(), at(0));
    state.move_(&buffer, LogicalMovement::DocEnd, false);
    assert_eq!(state.cursor(), buffer.len());
}

#[test]
fn home_on_an_already_home_cursor_stays_put() {
    let (buffer, mut state) = editor("aaa\nbbb");
    state.move_to(&buffer, at(4), false);
    state.move_(&buffer, LogicalMovement::LineStart, false);
    assert_eq!(state.cursor(), at(4));
    state.move_(&buffer, LogicalMovement::LineStart, false);
    assert_eq!(state.cursor(), at(4));
}

#[test]
fn both_line_ends_of_an_empty_line_are_the_same_offset() {
    let (buffer, mut state) = editor("a\n\nb");
    state.move_to(&buffer, at(2), false);
    state.move_(&buffer, LogicalMovement::LineEnd, false);
    assert_eq!(state.cursor(), at(2));
    state.move_(&buffer, LogicalMovement::LineStart, false);
    assert_eq!(state.cursor(), at(2));
}

#[test]
fn end_then_right_crosses_to_the_next_line() {
    let (buffer, mut state) = editor("aa\nbb");
    state.move_to(&buffer, at(0), false);
    state.move_(&buffer, LogicalMovement::LineEnd, false);
    assert_eq!(state.cursor(), at(2));
    state.move_(&buffer, LogicalMovement::Right, false);
    assert_eq!(state.cursor(), at(3));
}

#[test]
fn home_from_a_selection_uses_the_line_the_caret_is_on() {
    let (buffer, mut state) = editor("aaa\nbbb");
    state.move_to(&buffer, at(7), false);
    state.move_(&buffer, LogicalMovement::Left, true);
    state.move_(&buffer, LogicalMovement::LineStart, false);
    assert_eq!(state.cursor(), at(4));
}

#[test]
fn insert_splits_the_line_at_the_caret() {
    let (mut buffer, mut state) = editor("abcd");
    state.move_to(&buffer, at(2), false);
    state.insert(&mut buffer, Instant::now(), "\n");
    assert_eq!(buffer.text(), "ab\ncd");
    assert_eq!(state.cursor(), at(3));
    assert_eq!(buffer.line_count(), 2);
}

#[test]
fn insert_replaces_the_selection_in_one_undo_step() {
    let (mut buffer, mut state) = editor("abc");
    state.select_all(&buffer);
    state.insert(&mut buffer, Instant::now(), "\n");
    assert_eq!(buffer.text(), "\n");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "abc");
}

fn line(text: &str) -> Copied {
    Copied {
        text: text.to_string(),
        whole_line: true,
    }
}

fn span(text: &str) -> Copied {
    Copied {
        text: text.to_string(),
        whole_line: false,
    }
}

#[test]
fn copy_takes_exactly_the_selection() {
    let (buffer, mut state) = editor("abc");
    state.move_to(&buffer, at(0), false);
    state.move_to(&buffer, at(2), true);
    assert_eq!(state.copy(&buffer), span("ab"));
}

#[test]
fn copy_with_no_selection_takes_the_whole_line_and_its_newline() {
    let (buffer, mut state) = editor("x\ny\n");
    state.move_to(&buffer, at(0), false);
    assert_eq!(state.copy(&buffer), line("x\n"));
}

#[test]
fn copy_on_an_unterminated_last_line_still_ends_in_a_newline() {
    let (buffer, mut state) = editor("x\ny");
    state.move_to(&buffer, at(3), false);
    assert_eq!(state.copy(&buffer), line("y\n"));
}

#[test]
fn cut_removes_what_it_put_on_the_clipboard_in_one_undo_step() {
    let (mut buffer, mut state) = editor("abc");
    state.move_to(&buffer, at(0), false);
    state.move_to(&buffer, at(2), true);

    assert_eq!(state.cut(&mut buffer), span("ab"));
    assert_eq!(buffer.text(), "c");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "abc");
}

#[test]
fn cut_with_no_selection_removes_the_newline_too() {
    let (mut buffer, mut state) = editor("x\ny");
    state.move_to(&buffer, at(0), false);

    assert_eq!(state.cut(&mut buffer), line("x\n"));
    assert_eq!(buffer.text(), "y");
}

#[test]
fn cut_on_an_empty_last_line_copies_a_newline_and_removes_nothing() {
    let (mut buffer, mut state) = editor("x\n");
    state.move_to(&buffer, at(2), false);

    assert_eq!(state.cut(&mut buffer), line("\n"));
    assert_eq!(buffer.text(), "x\n");
    assert!(!state.undo(&mut buffer), "an empty removal is not an edit");
}

#[test]
fn a_cut_that_removes_nothing_does_not_split_a_typing_run() {
    let (mut buffer, mut state) = editor("x\n");
    let start = Instant::now();

    state.move_to(&buffer, at(0), false);
    state.insert(&mut buffer, start, "a");
    state.move_to(&buffer, buffer.len(), false);
    state.cut(&mut buffer);
    state.move_to(&buffer, at(1), false);
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2, "b");
    assert_eq!(buffer.text(), "abx\n");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "x\n", "both keystrokes were one run");
}

#[test]
fn pasting_a_span_replaces_the_selection() {
    let (mut buffer, mut state) = editor("abc");
    state.select_all(&buffer);
    state.paste(&mut buffer, &span("Z"));
    assert_eq!(buffer.text(), "Z");
}

#[test]
fn pasting_a_line_over_a_selection_replaces_it_like_any_other_text() {
    let (mut buffer, mut state) = editor("abc");
    state.select_all(&buffer);
    state.paste(&mut buffer, &line("Z\n"));
    assert_eq!(buffer.text(), "Z\n");
}

#[test]
fn pasting_a_line_at_a_bare_caret_inserts_above_and_keeps_the_column() {
    let (mut buffer, mut state) = editor("abc\ndef");
    state.move_to(&buffer, at(5), false);

    state.paste(&mut buffer, &line("XY\n"));
    assert_eq!(buffer.text(), "abc\nXY\ndef");
    assert_eq!(state.cursor(), at(8));
}

#[test]
fn a_paste_merges_with_the_typing_on_neither_side() {
    let (mut buffer, mut state) = editor("");
    let start = Instant::now();

    state.insert(&mut buffer, start, "a");
    state.paste(&mut buffer, &span("Z"));
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL, "b");
    assert_eq!(buffer.text(), "aZb");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "aZ", "the typing after it is its own step");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "a", "and so is the paste");
}

#[test]
fn a_cut_merges_with_the_typing_on_neither_side() {
    let (mut buffer, mut state) = editor("xy");
    let start = Instant::now();

    state.insert(&mut buffer, start, "a");
    state.move_(&buffer, LogicalMovement::Left, true);
    state.cut(&mut buffer);
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL, "b");
    assert_eq!(buffer.text(), "xyb");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "xy", "the typing after it is its own step");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "xya", "and so is the cut");
}

#[test]
fn a_paste_of_nothing_changes_nothing_and_splits_no_run() {
    let (mut buffer, mut state) = editor("");
    let start = Instant::now();

    state.insert(&mut buffer, start, "a");
    state.paste(&mut buffer, &span(""));
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2, "b");
    assert_eq!(buffer.text(), "ab");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "", "both keystrokes were one run");
}

#[test]
fn a_paste_of_nothing_keeps_the_selection_text() {
    let (mut buffer, mut state) = editor("abc");
    state.select_all(&buffer);
    state.paste(&mut buffer, &span(""));
    assert_eq!(buffer.text(), "abc");
}

#[test]
fn cutting_an_unterminated_last_line_and_pasting_it_back_adds_the_terminator() {
    // Pinned: the stated price of synthesizing the newline in `cut` as well as in `copy`.
    let (mut buffer, mut state) = editor("a");
    state.move_to(&buffer, at(0), false);

    let copied = state.cut(&mut buffer);
    assert_eq!(buffer.text(), "");
    state.paste(&mut buffer, &copied);
    assert_eq!(buffer.text(), "a\n");
}

#[test]
fn move_to_snaps_an_offset_off_a_char_boundary() {
    let (buffer, mut state) = editor("héllo");
    state.move_to(&buffer, at(2), false);
    assert_eq!(state.cursor(), at(1));
}

#[test]
fn move_vertically_moves_the_cursor_and_carries_the_goal() {
    let (buffer, mut state) = editor("ab");
    state.move_vertically(&buffer, at(1), false, Goal::Column(12.0));
    assert_eq!(state.cursor(), at(1));
    assert_eq!(state.selection().goal(), Goal::Column(12.0));
}

#[test]
fn move_to_clamps_an_offset_past_the_end() {
    let (buffer, mut state) = editor("ab");
    state.move_to(&buffer, at(99), false);
    assert_eq!(state.cursor(), buffer.len());
}

#[test]
fn an_explicit_range_outranks_the_composing_region_and_the_selection() {
    let (mut buffer, mut state) = editor("hello");
    state.edit(&mut buffer, Instant::now(), at(0)..at(0), "");
    state.set_marked_range(Some(at(1)..at(3)));

    assert_eq!(
        state.target_range(&buffer, Some(&(Utf16Offset::new(0)..Utf16Offset::new(2)))),
        at(0)..at(2)
    );
}

#[test]
fn the_composing_region_outranks_the_selection() {
    let (buffer, mut state) = editor("hello");
    state.move_to(&buffer, at(4), false);
    state.set_marked_range(Some(at(1)..at(3)));

    assert_eq!(state.target_range(&buffer, None), at(1)..at(3));
}

#[test]
fn the_selection_is_the_fallback() {
    let (buffer, mut state) = editor("hello");
    state.move_to(&buffer, at(1), false);
    state.move_to(&buffer, at(4), true);

    assert_eq!(state.target_range(&buffer, None), at(1)..at(4));
}

#[test]
fn a_utf16_range_spanning_a_surrogate_pair_resolves_onto_the_whole_char() {
    let (mut buffer, mut state) = editor("a😀b");
    state.replace_in_range(
        &mut buffer,
        Instant::now(),
        Some(&(Utf16Offset::new(1)..Utf16Offset::new(3))),
        "",
    );

    assert_eq!(buffer.text(), "ab");
    assert_eq!(state.cursor(), at(1));
}

#[test]
fn composing_text_marks_what_it_inserted() {
    let (mut buffer, mut state) = editor("");
    state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "^", None);

    assert_eq!(buffer.text(), "^");
    assert_eq!(state.marked_range(), Some(at(0)..at(1)));

    state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "ê", None);

    assert_eq!(buffer.text(), "ê", "the base letter replaces the accent");
    assert_eq!(state.marked_range(), Some(at(0)..at(2)));
}

#[test]
fn composing_with_empty_text_ends_the_composition() {
    let (mut buffer, mut state) = editor("");
    state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "^", None);
    state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "", None);

    assert_eq!(state.marked_range(), None);
}

/// Rebasing before conversion, not after: converting first would double-count the insertion point.
#[test]
fn the_composing_caret_is_rebased_before_it_is_converted() {
    let (mut buffer, mut state) = editor("😀");
    state.move_to(&buffer, buffer.len(), false);

    state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "ab", Some(0..1));

    assert_eq!(buffer.text(), "😀ab");
    assert_eq!(
        state.cursor(),
        at(5),
        "one char into `ab`, not one unit into the buffer"
    );
}
