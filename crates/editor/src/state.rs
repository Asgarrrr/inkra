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

/// What a copy took, and how a paste of it should land.
///
/// `whole_line` is carried rather than inferred from the text. Plenty of
/// ordinary clipboard content ends in a newline — a whole file, a command's
/// output, a paragraph from a web page — and none of it should be inserted
/// above the caret's line. The flag records how the text was *selected*, which
/// no inspection of the text can recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub text: String,
    /// The copy was a whole line: `text` ends with `\n`, and pasting it at a
    /// bare caret inserts a line above rather than splitting the current one.
    pub whole_line: bool,
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

    /// The range a platform input-method edit applies to, in the protocol's own
    /// order of precedence: an explicit range, else the composing region, else
    /// the selection.
    ///
    /// Here rather than at the gpui edge because it reads only a `Buffer` and a
    /// selection. Two views speak this protocol, and a second copy of the
    /// precedence rule is a second thing to get wrong.
    #[must_use]
    pub fn target_range(
        &self,
        buffer: &Buffer,
        range_utf16: Option<&Range<usize>>,
    ) -> Range<ByteOffset> {
        range_utf16
            .map(|range| buffer.from_utf16_range(range))
            .or_else(|| self.marked_range())
            .unwrap_or_else(|| self.selection.range())
    }

    /// Applies an input-method edit that carries no composing region — a
    /// committed character, or a paste from the platform.
    pub fn replace_in_range(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range_utf16: Option<&Range<usize>>,
        text: &str,
    ) {
        let range = self.target_range(buffer, range_utf16);
        self.edit(buffer, now, range, text);
    }

    /// The same, for text still being composed: a dead key waiting for its base
    /// letter, or an IME's provisional reading.
    ///
    /// `selected_utf16` is where the input method wants the caret, **relative to
    /// the text it just inserted**. It is rebased while still in UTF-16 and
    /// converted once: converting first would resolve it against the whole
    /// buffer, and adding the insertion point afterwards would count the offset
    /// twice.
    pub fn mark_and_replace_in_range(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range_utf16: Option<&Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) {
        let range = self.target_range(buffer, range_utf16);
        let start = range.start;
        self.edit(buffer, now, range, text);

        // `edit` clears the composing region, so this reinstates it over what
        // was just inserted. Empty text ends the composition rather than
        // marking a zero-width region.
        self.set_marked_range((!text.is_empty()).then(|| start..ByteOffset(start.0 + text.len())));

        if let Some(selected) = selected_utf16 {
            let inserted_at = buffer.to_utf16(start).0;
            let cursor = buffer.from_utf16(inkra_text::Utf16Offset(inserted_at + selected.end));
            self.move_to(buffer, cursor, false);
        }
    }

    /// What `⌘C` takes. Reads only: a copy owes no repaint and must not mark
    /// the note dirty.
    #[must_use]
    pub fn copy(&self, buffer: &Buffer) -> Copied {
        self.target(buffer).1
    }

    /// [`EditorState::copy`], then removes what it took.
    ///
    /// Sealed on both sides, like [`EditorState::paste`]: a cut is a gesture of
    /// its own and must not swallow the typing around it.
    pub fn cut(&mut self, buffer: &mut Buffer, now: Instant) -> Copied {
        let (range, copied) = self.target(buffer);
        // An empty range is the empty last line: there is a line to copy but no
        // byte to remove. Editing anyway would leave `grouping_blocked` set by
        // a transaction that never lands.
        if !range.is_empty() {
            buffer.finalize_transaction();
            self.edit(buffer, now, range, "");
            buffer.finalize_transaction();
        }
        copied
    }

    /// What `⌘V` does.
    ///
    /// Finalizing on *both* sides is load-bearing. [`Buffer::close`] clears the
    /// flag the moment it pushes a transaction, so sealing only beforehand
    /// leaves the next keystroke free to merge into the paste. Zed avoids this
    /// by pasting through the ungrouped `Buffer::transact`; every mutation here
    /// carries a `now` and groups.
    pub fn paste(&mut self, buffer: &mut Buffer, now: Instant, copied: &Copied) {
        // Before the seal, not after: a paste of nothing must leave the
        // surrounding typing run intact rather than splitting it in two.
        if copied.text.is_empty() {
            return;
        }

        buffer.finalize_transaction();
        if copied.whole_line && self.selection.is_empty() {
            let caret = self.cursor();
            let start = self.line(buffer).start;
            self.edit(buffer, now, start..start, &copied.text);
            // The insertion sits entirely before the caret, so the caret keeps
            // its column on the line that moved down. `edit` would have left it
            // at the start of that line instead.
            self.selection
                .collapse_to(ByteOffset(caret.0 + copied.text.len()), Goal::None);
        } else {
            self.insert(buffer, now, &copied.text);
        }
        buffer.finalize_transaction();
    }

    /// The range a copy or a cut acts on, and what it puts on the clipboard.
    ///
    /// The two differ only on a last line with no terminator: the range stops
    /// at the end of the text, while the text gains the `\n` the line does not
    /// have. That asymmetry is deliberate and is where we part company with
    /// Zed, whose cut copies exactly the bytes it removes. Ours guarantees that
    /// a whole-line item is a whole line wherever it lands — which is what
    /// cutting a line is for — and pays for it by giving an unterminated last
    /// line its terminator when the cut is pasted straight back.
    fn target(&self, buffer: &Buffer) -> (Range<ByteOffset>, Copied) {
        if !self.selection.is_empty() {
            let range = self.selection.range();
            let text = buffer.text()[range.start.0..range.end.0].to_string();
            return (
                range,
                Copied {
                    text,
                    whole_line: false,
                },
            );
        }

        let line = self.line(buffer);
        // `line_range` stops before the newline, so anything left after it is
        // one, and there is exactly one byte of it.
        let terminated = line.end < buffer.len();
        let end = if terminated {
            ByteOffset(line.end.0 + 1)
        } else {
            line.end
        };
        let mut text = buffer.text()[line.start.0..end.0].to_string();
        if !terminated {
            text.push('\n');
        }

        (
            line.start..end,
            Copied {
                text,
                whole_line: true,
            },
        )
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
        // The synthesized newline is what makes the item a whole line wherever
        // it is pasted, rather than one that welds itself onto another.
        let (buffer, mut state) = editor("x\ny");
        state.move_to(&buffer, at(3), false);
        assert_eq!(state.copy(&buffer), line("y\n"));
    }

    #[test]
    fn cut_removes_what_it_put_on_the_clipboard_in_one_undo_step() {
        let (mut buffer, mut state) = editor("abc");
        state.move_to(&buffer, at(0), false);
        state.move_to(&buffer, at(2), true);

        assert_eq!(state.cut(&mut buffer, Instant::now()), span("ab"));
        assert_eq!(buffer.text(), "c");

        assert!(state.undo(&mut buffer));
        assert_eq!(buffer.text(), "abc");
    }

    #[test]
    fn cut_with_no_selection_removes_the_newline_too() {
        let (mut buffer, mut state) = editor("x\ny");
        state.move_to(&buffer, at(0), false);

        assert_eq!(state.cut(&mut buffer, Instant::now()), line("x\n"));
        assert_eq!(buffer.text(), "y");
    }

    #[test]
    fn cut_on_an_empty_last_line_copies_a_newline_and_removes_nothing() {
        let (mut buffer, mut state) = editor("x\n");
        state.move_to(&buffer, at(2), false);

        assert_eq!(state.cut(&mut buffer, Instant::now()), line("\n"));
        assert_eq!(buffer.text(), "x\n");
        assert!(!state.undo(&mut buffer), "an empty removal is not an edit");
    }

    #[test]
    fn a_cut_that_removes_nothing_does_not_split_a_typing_run() {
        // The seal is skipped along with the edit. Left set by a transaction
        // that never lands, `grouping_blocked` would break the next run apart.
        let (mut buffer, mut state) = editor("x\n");
        let start = Instant::now();

        state.move_to(&buffer, at(0), false);
        state.insert(&mut buffer, start, "a");
        state.move_to(&buffer, buffer.len(), false);
        state.cut(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2);
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
        state.paste(&mut buffer, Instant::now(), &span("Z"));
        assert_eq!(buffer.text(), "Z");
    }

    #[test]
    fn pasting_a_line_over_a_selection_replaces_it_like_any_other_text() {
        let (mut buffer, mut state) = editor("abc");
        state.select_all(&buffer);
        state.paste(&mut buffer, Instant::now(), &line("Z\n"));
        assert_eq!(buffer.text(), "Z\n");
    }

    #[test]
    fn pasting_a_line_at_a_bare_caret_inserts_above_and_keeps_the_column() {
        let (mut buffer, mut state) = editor("abc\ndef");
        // Line 1, column 1: between 'd' and 'e'.
        state.move_to(&buffer, at(5), false);

        state.paste(&mut buffer, Instant::now(), &line("XY\n"));
        assert_eq!(buffer.text(), "abc\nXY\ndef");
        // Still between 'd' and 'e', now one line further down. Left to `edit`,
        // the caret would have landed at offset 7, the start of "def".
        assert_eq!(state.cursor(), at(8));
    }

    #[test]
    fn a_paste_merges_with_the_typing_on_neither_side() {
        let (mut buffer, mut state) = editor("");
        let start = Instant::now();

        state.insert(&mut buffer, start, "a");
        state.paste(
            &mut buffer,
            start + inkra_text::GROUP_INTERVAL / 2,
            &span("Z"),
        );
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
        state.cut(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2);
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
        state.paste(
            &mut buffer,
            start + inkra_text::GROUP_INTERVAL / 2,
            &span(""),
        );
        state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2, "b");
        assert_eq!(buffer.text(), "ab");

        assert!(state.undo(&mut buffer));
        assert_eq!(buffer.text(), "", "both keystrokes were one run");
    }

    #[test]
    fn cutting_an_unterminated_last_line_and_pasting_it_back_adds_the_terminator() {
        // The stated price of synthesizing the newline in `cut` as well as in
        // `copy`. Pinned so it cannot change without someone deciding to.
        let (mut buffer, mut state) = editor("a");
        state.move_to(&buffer, at(0), false);

        let copied = state.cut(&mut buffer, Instant::now());
        assert_eq!(buffer.text(), "");
        state.paste(&mut buffer, Instant::now(), &copied);
        assert_eq!(buffer.text(), "a\n");
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

    /// The protocol's precedence, which used to live at the gpui edge and so
    /// needed a window to reach. Three cases, in the order the rule states
    /// them.
    #[test]
    fn an_explicit_range_outranks_the_composing_region_and_the_selection() {
        let (mut buffer, mut state) = editor("hello");
        state.edit(&mut buffer, Instant::now(), at(0)..at(0), "");
        state.set_marked_range(Some(at(1)..at(3)));

        assert_eq!(state.target_range(&buffer, Some(&(0..2))), at(0)..at(2));
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

    /// The protocol counts UTF-16 units, so a range spanning a surrogate pair
    /// must resolve onto whole characters.
    #[test]
    fn a_utf16_range_spanning_a_surrogate_pair_resolves_onto_the_whole_char() {
        // 'a'@0, the emoji @1..5 as two UTF-16 units, 'b'@5.
        let (mut buffer, mut state) = editor("a😀b");
        state.replace_in_range(&mut buffer, Instant::now(), Some(&(1..3)), "");

        assert_eq!(buffer.text(), "ab");
        assert_eq!(state.cursor(), at(1));
    }

    /// A dead key: `^` arrives as composing text, then its base letter replaces
    /// it. The region has to be reinstated after the edit, because `edit`
    /// clears it.
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

    /// Committing ends the composition rather than marking a zero-width region.
    #[test]
    fn composing_with_empty_text_ends_the_composition() {
        let (mut buffer, mut state) = editor("");
        state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "^", None);
        state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "", None);

        assert_eq!(state.marked_range(), None);
    }

    /// The input method reports the caret relative to what it just inserted.
    /// Converting before rebasing would resolve it against the whole buffer and
    /// count the insertion point twice — which only shows up when the text does
    /// not start at zero, and when a character is more than one UTF-16 unit.
    #[test]
    fn the_composing_caret_is_rebased_before_it_is_converted() {
        let (mut buffer, mut state) = editor("😀");
        state.move_to(&buffer, buffer.len(), false);

        // Insert at offset 4 (two UTF-16 units in), with the caret one unit
        // into the inserted text.
        state.mark_and_replace_in_range(&mut buffer, Instant::now(), None, "ab", Some(0..1));

        assert_eq!(buffer.text(), "😀ab");
        assert_eq!(
            state.cursor(),
            at(5),
            "one char into `ab`, not one unit into the buffer"
        );
    }
}
