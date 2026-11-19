use std::ops::Range;
use std::time::Instant;

use inkra_text::{Bias, Buffer, ByteOffset, Diff, Utf16Offset};

use crate::selection::{Goal, Selection};

pub use clipboard::Copied;

mod clipboard;

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

/// The snap of a host whose caret may rest at every offset.
#[must_use]
pub fn no_snap(offset: ByteOffset, _: Bias) -> ByteOffset {
    offset
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

    /// Every grouped buffer mutation goes through here: one gesture is one undo step, and the composing region clears.
    pub fn edit(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        range: Range<ByteOffset>,
        text: &str,
    ) {
        let start = range.start;
        buffer.transact_at(now, |buffer| buffer.replace(range, text));
        self.edited(start, text);
    }

    /// `edit` as one undo step that merges with the typing on neither side.
    fn edit_ungrouped(&mut self, buffer: &mut Buffer, range: Range<ByteOffset>, text: &str) {
        let start = range.start;
        buffer.transact(|buffer| buffer.replace(range, text));
        self.edited(start, text);
    }

    fn edited(&mut self, start: ByteOffset, text: &str) {
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
            // On the new text, which the host snaps once it parses it.
            self.move_to(buffer, cursor, false, no_snap);
        }
    }

    /// `snap` moves an offset where no caret may rest to the edge its bias picks, so a step over
    /// such a run removes it whole.
    pub fn backspace(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        self.remove(buffer, now, Direction::Backward, snap);
    }

    pub fn delete(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        self.remove(buffer, now, Direction::Forward, snap);
    }

    fn remove(
        &mut self,
        buffer: &mut Buffer,
        now: Instant,
        direction: Direction,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        let range = if self.selection.is_empty() {
            let cursor = self.cursor();
            match direction {
                Direction::Backward => snap(buffer.previous_grapheme(cursor), Bias::Left)..cursor,
                Direction::Forward => cursor..snap(buffer.next_grapheme(cursor), Bias::Right),
            }
        } else {
            self.selection.range()
        };

        if !range.is_empty() {
            self.edit(buffer, now, range, "");
        }
    }

    /// `snap` as `backspace` takes it: a step left lands at the run's start, any other at its end.
    pub fn move_(
        &mut self,
        buffer: &Buffer,
        movement: LogicalMovement,
        extend: bool,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
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
        let bias = match movement {
            LogicalMovement::Left => Bias::Left,
            _ => Bias::Right,
        };
        self.place(buffer, target, bias, extend, Goal::None, snap);
    }

    fn line(&self, buffer: &Buffer) -> Range<ByteOffset> {
        buffer.line_at(self.cursor())
    }

    /// Clears the goal column; a vertical move that must keep one uses `move_vertically` instead.
    /// Lands past a run `snap` forbids, as Home lands at a line's text.
    pub fn move_to(
        &mut self,
        buffer: &Buffer,
        offset: ByteOffset,
        extend: bool,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        self.place(buffer, offset, Bias::Right, extend, Goal::None, snap);
    }

    pub fn move_vertically(
        &mut self,
        buffer: &Buffer,
        offset: ByteOffset,
        extend: bool,
        goal: Goal,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        self.place(buffer, offset, Bias::Right, extend, goal, snap);
    }

    fn place(
        &mut self,
        buffer: &Buffer,
        offset: ByteOffset,
        bias: Bias,
        extend: bool,
        goal: Goal,
        snap: impl Fn(ByteOffset, Bias) -> ByteOffset,
    ) {
        let offset = buffer.clip(snap(offset, bias));
        if extend {
            self.selection.set_head(offset, goal);
        } else {
            self.selection.collapse_to(offset, goal);
        }
    }

    /// After the text changed: a caret lands at the end of a run `snap` forbids, a range widens
    /// over it at both ends.
    pub fn snap(&mut self, snap: impl Fn(ByteOffset, Bias) -> ByteOffset) {
        let range = self.selection.range();
        let snapped = if range.is_empty() {
            let caret = snap(range.start, Bias::Right);
            caret..caret
        } else {
            snap(range.start, Bias::Left)..snap(range.end, Bias::Right)
        };
        if snapped != range {
            self.selection = Selection::new(snapped, self.selection.reversed());
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

    /// Applies `diff` as one undo step of its own and maps the selection through it; false, with
    /// nothing changed, when the buffer moved past `diff`'s base.
    pub fn apply_diff(&mut self, buffer: &mut Buffer, diff: &Diff) -> bool {
        let applied = buffer.apply_diff(diff);
        if applied {
            self.map_through(diff);
        }
        applied
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

#[cfg(test)]
mod apply_tests;

#[cfg(test)]
mod snap_tests;
