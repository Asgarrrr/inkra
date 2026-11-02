use std::ops::Range;

use gpui::{Context, Pixels, Window, px};
use inkra_editor::{Goal, LogicalMovement};
use inkra_text::{Bias, ByteOffset};

use super::EditorView;
use super::layout::DocumentLayout;
use super::viewport::Reveal;
use crate::text_input::InputHost;

#[derive(Debug, Clone, Copy)]
pub(super) enum Vertical {
    Up,
    Down,
}

impl EditorView {
    /// Snapped outward to graphemes: the range is in the saved text, which unsaved edits may move.
    pub(crate) fn select_range(
        &mut self,
        range: Range<ByteOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, buffer.clip_grapheme(range.start, Bias::Left), false);
            state.move_to(buffer, buffer.clip_grapheme(range.end, Bias::Right), true);
        });
        // Over the gate's minimal reveal: the last reveal decides the offset, before any paint.
        self.reveal_cursor(Reveal::Center);
    }

    pub(super) fn move_vertically(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        direction: Vertical,
        extend: bool,
    ) {
        let Some(layout) = self.input.last_layout() else {
            return;
        };
        let goal = self.input.state().selection().goal();
        if let Some((offset, goal)) = vertical(layout.as_ref(), self.cursor(), goal, direction) {
            self.move_cursor(window, cx, |state, buffer| {
                state.move_vertically(buffer, offset, extend, goal);
            });
            return;
        }

        // Drops the goal: the next vertical move starts from where this landed, not where it aimed.
        let movement = match direction {
            Vertical::Up => LogicalMovement::DocStart,
            Vertical::Down => LogicalMovement::DocEnd,
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_(buffer, movement, extend);
        });
    }
}

/// What `vertical` reads from a layout.
pub(super) trait CaretRows {
    fn row_count(&self) -> usize;
    fn row_for_offset(&self, offset: ByteOffset) -> usize;
    fn holds_caret(&self, row: usize) -> bool;
    fn x_for_offset(&self, offset: ByteOffset) -> Pixels;
    fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset;
}

impl CaretRows for DocumentLayout {
    fn row_count(&self) -> usize {
        self.row_count()
    }

    fn row_for_offset(&self, offset: ByteOffset) -> usize {
        self.row_for_offset(offset)
    }

    fn holds_caret(&self, row: usize) -> bool {
        self.holds_caret(row)
    }

    fn x_for_offset(&self, offset: ByteOffset) -> Pixels {
        self.x_for_offset(offset)
    }

    fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset {
        self.offset_on_row(row, x)
    }
}

/// The offset and goal one caret row up or down; `None` past the first or last row.
pub(super) fn vertical(
    rows: &impl CaretRows,
    cursor: ByteOffset,
    goal: Goal,
    direction: Vertical,
) -> Option<(ByteOffset, Goal)> {
    // Reused verbatim: re-widens a run of downs after a short row (x is
    // from the row's start, so it means the same on every row).
    let goal_x = match goal {
        Goal::Column(x) => px(x),
        Goal::None => rows.x_for_offset(cursor),
    };
    let step = |row: usize| match direction {
        Vertical::Up => row.checked_sub(1),
        Vertical::Down => (row + 1 < rows.row_count()).then_some(row + 1),
    };
    let mut target = step(rows.row_for_offset(cursor))?;
    while !rows.holds_caret(target) {
        target = step(target)?;
    }
    Some((
        rows.offset_on_row(target, goal_x),
        Goal::Column(f32::from(goal_x)),
    ))
}

#[cfg(test)]
mod tests;
