//! Editing as a gesture: what a keystroke, a movement or a delete does to the
//! text and the caret.
//!
//! Everything here is resolvable without metrics. A movement that needs to know
//! how tall a line is — up, down, a click — stops at `LogicalMovement` and is
//! finished in `app`, which is the layer that has a font.

use std::ops::Range;
use std::time::Instant;

use inkra_text::{Buffer, ByteOffset};

use crate::selection::{Goal, Selection};

/// Movements resolvable from the text alone.
///
/// Up, down and click-to-offset are deliberately absent: they need font
/// metrics, so they are resolved in the layer that has them and arrive here as
/// [`EditorState::move_to`]. [`LogicalMovement::LineStart`] and `LineEnd` are
/// *logical* line ends — with soft wrap they would stop matching the visible
/// ends of a wrapped row, and would move up here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalMovement {
    Left,
    Right,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Backward,
    Forward,
}

/// One view's editing state over a buffer it does not own.
///
/// The buffer arrives per operation rather than being held, because a note
/// open in two panes is one buffer and two of these. Note that two panes are
/// not yet *correct* — see the design document: a second view's offsets rot
/// when the first one edits, and fixing that needs anchors.
///
/// Nothing here reads a clock. The caller supplies `now`, for the same reason
/// [`Buffer::transact_at`] does.
#[derive(Debug, Clone)]
pub struct EditorState {
    selection: Selection,
    marked_range: Option<Range<ByteOffset>>,
}

impl EditorState {
    #[must_use]
    pub fn new(selection: Selection) -> Self {
        Self {
            selection,
            marked_range: None,
        }
    }

    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    #[must_use]
    pub fn cursor(&self) -> ByteOffset {
        self.selection.head()
    }

    /// The region the input method is composing into, if any.
    #[must_use]
    pub fn marked_range(&self) -> Option<Range<ByteOffset>> {
        self.marked_range.clone()
    }

    pub fn set_marked_range(&mut self, range: Option<Range<ByteOffset>>) {
        self.marked_range = range;
    }

    /// Every buffer mutation goes through here, so one user gesture is one undo
    /// step even when it deletes and inserts, and a continuous typing run
    /// collapses into a single step.
    ///
    /// Clears the composing region: any edit invalidates it. A caller that is
    /// itself mid-composition sets the new region afterwards.
    pub fn edit(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range: Range<ByteOffset>,
        text: &str,
    ) {
        let start = range.start;
        buffer.transact_at(now, |buffer| buffer.replace(range, text));
        // The goal goes with the composing region: an edit moves the text under
        // the caret, so a column remembered before it no longer describes it.
        self.selection
            .collapse_to(ByteOffset(start.0 + text.len()), Goal::None);
        self.marked_range = None;
    }

    /// Replaces the selection with `text`, or inserts at the caret when there is
    /// none — what typing a character does, and what pasting will do.
    pub fn insert(&mut self, buffer: &mut Buffer, now: Instant, text: &str) {
        let range = self.selection.range();
        self.edit(buffer, now, range, text);
    }

    pub fn backspace(&mut self, buffer: &mut Buffer, now: Instant) {
        self.remove(buffer, now, Direction::Backward);
    }

    pub fn delete(&mut self, buffer: &mut Buffer, now: Instant) {
        self.remove(buffer, now, Direction::Forward);
    }

    /// Backspace and delete differ only in which way an empty selection
    /// reaches for a grapheme.
    fn remove(&mut self, buffer: &mut Buffer, now: Instant, direction: Direction) {
        let range = if self.selection.is_empty() {
            let cursor = self.cursor();
            match direction {
                Direction::Backward => buffer.previous_grapheme(cursor)..cursor,
                Direction::Forward => cursor..buffer.next_grapheme(cursor),
            }
        } else {
            self.selection.range()
        };

        if !range.is_empty() {
            self.edit(buffer, now, range, "");
        }
    }

    pub fn move_(&mut self, buffer: &Buffer, movement: LogicalMovement, extend: bool) {
        // Collapsing a selection with a bare arrow lands on the edge you moved
        // towards; extending always steps from the moving end.
        let collapsing = !extend && !self.selection.is_empty();
        let target = match movement {
            LogicalMovement::Left if collapsing => self.selection.start(),
            LogicalMovement::Right if collapsing => self.selection.end(),
            LogicalMovement::Left => buffer.previous_grapheme(self.cursor()),
            LogicalMovement::Right => buffer.next_grapheme(self.cursor()),
            // From the head, so Home after a shift-selection lands on the line
            // the caret is on rather than the one the anchor is on.
            LogicalMovement::LineStart => self.line(buffer).start,
            LogicalMovement::LineEnd => self.line(buffer).end,
            LogicalMovement::DocStart => ByteOffset(0),
            LogicalMovement::DocEnd => buffer.len(),
        };
        self.move_to(buffer, target, extend);
    }

    /// The logical line the caret is on, excluding its terminating newline.
    fn line(&self, buffer: &Buffer) -> Range<ByteOffset> {
        buffer.line_range(buffer.line_for_offset(self.cursor()))
    }

    /// The entry point for movements resolved elsewhere — a mouse click, or a
    /// wrap-aware line end once soft wrap exists.
    ///
    /// Clears the goal column. Anything that must keep one is a vertical move
    /// and uses [`EditorState::move_vertically`].
    pub fn move_to(&mut self, buffer: &Buffer, offset: ByteOffset, extend: bool) {
        self.place(buffer, offset, extend, Goal::None);
    }

    /// Up and down. They need font metrics, so the caller resolves both the
    /// target offset and the column to remember; this is the one entry point
    /// that carries a goal through instead of dropping it.
    pub fn move_vertically(
        &mut self,
        buffer: &Buffer,
        offset: ByteOffset,
        extend: bool,
        goal: Goal,
    ) {
        self.place(buffer, offset, extend, goal);
    }

    fn place(&mut self, buffer: &Buffer, offset: ByteOffset, extend: bool, goal: Goal) {
        let offset = buffer.clip(offset);
        if extend {
            self.selection.set_head(offset, goal);
        } else {
            self.selection.collapse_to(offset, goal);
        }
    }

    pub fn select_all(&mut self, buffer: &Buffer) {
        self.selection = Selection::new(ByteOffset(0)..buffer.len(), false);
    }

    /// Returns whether anything was undone.
    ///
    /// Not a repaint gate: the app's mutation path notifies unconditionally,
    /// which costs a wasted frame on an empty history and buys a repaint that
    /// cannot be forgotten. The bool distinguishes "nothing to undo" from a
    /// real undo for callers that need to tell them apart.
    pub fn undo(&mut self, buffer: &mut Buffer) -> bool {
        let Some(range) = buffer.undo() else {
            return false;
        };
        self.settle_on(buffer, range.end);
        true
    }

    /// Returns whether anything was redone.
    pub fn redo(&mut self, buffer: &mut Buffer) -> bool {
        let Some(range) = buffer.redo() else {
            return false;
        };
        self.settle_on(buffer, range.end);
        true
    }

    /// Places the cursor on text that just moved under it, and drops a
    /// composing region that no longer describes anything.
    fn settle_on(&mut self, buffer: &Buffer, offset: ByteOffset) {
        self.selection.collapse_to(buffer.clip(offset), Goal::None);
        self.marked_range = None;
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn at(offset: usize) -> ByteOffset {
        ByteOffset(offset)
    }

    fn editor(text: &str) -> (Buffer, EditorState) {
        let buffer = Buffer::new(text);
        let state = EditorState::new(Selection::at(buffer.len()));
        (buffer, state)
    }

    #[test]
    fn backspace_deletes_a_grapheme_not_a_char() {
        // "e" plus a combining acute is two chars but one grapheme.
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
        // The newline separates the lines rather than belonging to either, so
        // the end of one and the start of the next are one grapheme apart.
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

    #[test]
    fn move_to_snaps_an_offset_off_a_char_boundary() {
        // "héllo": 'é' spans bytes 1..3, so offset 2 splits it.
        let (buffer, mut state) = editor("héllo");
        state.move_to(&buffer, at(2), false);
        assert_eq!(state.cursor(), at(1));
    }

    #[test]
    fn move_to_clamps_an_offset_past_the_end() {
        let (buffer, mut state) = editor("ab");
        state.move_to(&buffer, at(99), false);
        assert_eq!(state.cursor(), buffer.len());
    }
}
