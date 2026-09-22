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
/// open in two panes is one buffer and two of these. Two panes are not yet
/// correct (see CLAUDE.md): a second view's offsets rot when the first
/// edits.
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
    /// have — unlike Zed, whose cut copies exactly the removed bytes. This
    /// guarantees a whole-line item is a whole line wherever it lands, at the
    /// cost of adding a terminator when the cut is pasted straight back.
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
mod tests;
