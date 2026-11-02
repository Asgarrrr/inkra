use std::ops::Range;
use std::time::Instant;

use inkra_text::{Bias, Buffer, ByteOffset, Diff, Utf16Offset};

use crate::selection::{Goal, Selection};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub text: String,
    pub whole_line: bool,
}

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

    #[must_use]
    pub fn marked_range(&self) -> Option<Range<ByteOffset>> {
        self.marked_range.clone()
    }

    pub fn set_marked_range(&mut self, range: Option<Range<ByteOffset>>) {
        self.marked_range = range;
    }

    /// Every buffer mutation goes through here: one gesture is one undo step, and the composing region clears.
    pub fn edit(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range: Range<ByteOffset>,
        text: &str,
    ) {
        let start = range.start;
        buffer.transact_at(now, |buffer| buffer.replace(range, text));
        // An edit moves the text under the caret, so a remembered column no longer describes it.
        self.selection.collapse_to(start + text.len(), Goal::None);
        self.marked_range = None;
    }

    pub fn insert(&mut self, buffer: &mut Buffer, now: Instant, text: &str) {
        let range = self.selection.range();
        self.edit(buffer, now, range, text);
    }

    #[must_use]
    pub fn target_range(
        &self,
        buffer: &Buffer,
        range_utf16: Option<&Range<Utf16Offset>>,
    ) -> Range<ByteOffset> {
        range_utf16
            .map(|range| buffer.from_utf16_range(range))
            .or_else(|| self.marked_range())
            .unwrap_or_else(|| self.selection.range())
    }

    pub fn replace_in_range(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range_utf16: Option<&Range<Utf16Offset>>,
        text: &str,
    ) {
        let range = self.target_range(buffer, range_utf16);
        self.edit(buffer, now, range, text);
    }

    /// `selected_utf16` is rebased in UTF-16 before converting once; converting first would double-count it.
    pub fn mark_and_replace_in_range(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range_utf16: Option<&Range<Utf16Offset>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) {
        let range = self.target_range(buffer, range_utf16);
        let start = range.start;
        self.edit(buffer, now, range, text);

        self.set_marked_range((!text.is_empty()).then(|| start..start + text.len()));

        if let Some(selected) = selected_utf16 {
            let inserted_at = buffer.to_utf16(start);
            let cursor = buffer.from_utf16(inserted_at + selected.end);
            self.move_to(buffer, cursor, false);
        }
    }

    /// What `⌘C` takes. Read-only: a copy must not repaint or mark the note dirty.
    #[must_use]
    pub fn copy(&self, buffer: &Buffer) -> Copied {
        self.target(buffer).1
    }

    /// `copy`, then removes what it took; sealed on both sides so it can't swallow adjacent typing.
    pub fn cut(&mut self, buffer: &mut Buffer, now: Instant) -> Copied {
        let (range, copied) = self.target(buffer);
        // An empty range is the empty last line: nothing to remove, so skip the edit — otherwise
        // `grouping_blocked` is set by a transaction that never lands.
        if !range.is_empty() {
            buffer.finalize_transaction();
            self.edit(buffer, now, range, "");
            buffer.finalize_transaction();
        }
        copied
    }

    /// Finalizing on *both* sides is load-bearing: `close` clears the flag on push, so sealing
    /// only beforehand leaves the next keystroke free to merge into the paste.
    pub fn paste(&mut self, buffer: &mut Buffer, now: Instant, copied: &Copied) {
        // Before the seal: a paste of nothing must not split the surrounding typing run.
        if copied.text.is_empty() {
            return;
        }

        buffer.finalize_transaction();
        if copied.whole_line && self.selection.is_empty() {
            let caret = self.cursor();
            let start = self.line(buffer).start;
            self.edit(buffer, now, start..start, &copied.text);
            // The insertion sits before the caret, so it keeps its column; `edit` alone would reset it to line start.
            self.selection
                .collapse_to(caret + copied.text.len(), Goal::None);
        } else {
            self.insert(buffer, now, &copied.text);
        }
        buffer.finalize_transaction();
    }

    /// On an unterminated last line, the text gains a `\n` the range doesn't cover, so a
    /// whole-line item is always a whole line, even pasted straight back.
    fn target(&self, buffer: &Buffer) -> (Range<ByteOffset>, Copied) {
        if !self.selection.is_empty() {
            let range = self.selection.range();
            let text = buffer.slice(range.start..range.end).to_string();
            return (
                range,
                Copied {
                    text,
                    whole_line: false,
                },
            );
        }

        let line = self.line(buffer);
        // `line_range` stops before the newline, so at most one terminator byte follows it.
        let terminated = line.end < buffer.len();
        let end = if terminated { line.end + 1 } else { line.end };
        let mut text = buffer.slice(line.start..end).to_string();
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
        let collapsing = !extend && !self.selection.is_empty();
        let target = match movement {
            LogicalMovement::Left if collapsing => self.selection.start(),
            LogicalMovement::Right if collapsing => self.selection.end(),
            LogicalMovement::Left => buffer.previous_grapheme(self.cursor()),
            LogicalMovement::Right => buffer.next_grapheme(self.cursor()),
            // From the head, so Home after a shift-selection lands on the caret's line, not the anchor's.
            LogicalMovement::LineStart => self.line(buffer).start,
            LogicalMovement::LineEnd => self.line(buffer).end,
            LogicalMovement::DocStart => ByteOffset::new(0),
            LogicalMovement::DocEnd => buffer.len(),
        };
        self.move_to(buffer, target, extend);
    }

    fn line(&self, buffer: &Buffer) -> Range<ByteOffset> {
        buffer.line_range(buffer.line_for_offset(self.cursor()))
    }

    /// Clears the goal column; a vertical move that must keep one uses `move_vertically` instead.
    pub fn move_to(&mut self, buffer: &Buffer, offset: ByteOffset, extend: bool) {
        self.place(buffer, offset, extend, Goal::None);
    }

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
        self.selection = Selection::new(ByteOffset::new(0)..buffer.len(), false);
    }

    /// Not a repaint gate: the app notifies unconditionally regardless; the bool only tells callers apart.
    pub fn undo(&mut self, buffer: &mut Buffer) -> bool {
        let Some(range) = buffer.undo() else {
            return false;
        };
        self.settle_on(buffer, range.end);
        true
    }

    pub fn redo(&mut self, buffer: &mut Buffer) -> bool {
        let Some(range) = buffer.redo() else {
            return false;
        };
        self.settle_on(buffer, range.end);
        true
    }

    /// Follows a change made outside the editor, with Zed's biases: a caret lands after a
    /// replacement, and a range never takes in text inserted at its edges.
    pub fn map_through(&mut self, diff: &Diff) {
        if self.selection.is_empty() {
            let caret = diff.map(self.selection.start(), Bias::Right);
            self.selection.collapse_to(caret, Goal::None);
        } else {
            let range = map_inward(diff, &self.selection.range());
            self.selection = Selection::settled(range, self.selection.reversed());
        }
        self.marked_range = self
            .marked_range
            .as_ref()
            .map(|marked| map_inward(diff, marked))
            .filter(|marked| !marked.is_empty());
    }

    /// Places the cursor on text that moved under it, and drops a composing region that no longer applies.
    fn settle_on(&mut self, buffer: &Buffer, offset: ByteOffset) {
        self.selection.collapse_to(buffer.clip(offset), Goal::None);
        self.marked_range = None;
    }
}

/// Start `Right`, end `Left`: the range keeps its ends on their own text, and may cross.
fn map_inward(diff: &Diff, range: &Range<ByteOffset>) -> Range<ByteOffset> {
    diff.map(range.start, Bias::Right)..diff.map(range.end, Bias::Left)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod map_tests;
