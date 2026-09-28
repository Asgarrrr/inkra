use std::ops::Range;

use gpui::{Context, Window, point, px};
use inkra_editor::{EditorState, Goal, LogicalMovement};
use inkra_markdown::touched;
use inkra_text::{Bias, Buffer, ByteOffset};

use super::EditorView;
use super::scroll::{Reveal, ScrollTo};

#[derive(Debug, Clone, Copy)]
pub(super) enum Vertical {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RowEdge {
    Start,
    End,
}

impl EditorView {
    /// Reshapes when the move reveals or hides an element, so `last_layout` is never stale (spec D7).
    pub(super) fn move_cursor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &Buffer),
    ) {
        self.move_cursor_revealing(Reveal::Minimal, window, cx, f);
    }

    fn move_cursor_revealing(
        &mut self,
        how: Reveal,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &Buffer),
    ) {
        let old = self.state.selection().range();
        f(&mut self.state, self.buffer.read(cx));
        let new = self.state.selection().range();
        let conceals = &self.markdown.conceals;
        if !touched(conceals, &old).eq(touched(conceals, &new)) {
            self.reshape(window, cx);
        }
        cx.notify();
        self.reveal_cursor(how);
    }

    /// Snapped outward to graphemes: the range is in the saved text, which unsaved edits may move.
    pub(crate) fn select_range(
        &mut self,
        range: Range<ByteOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursor_revealing(Reveal::Center, window, cx, |state, buffer| {
            state.move_to(buffer, buffer.clip_grapheme(range.start, Bias::Left), false);
            state.move_to(buffer, buffer.clip_grapheme(range.end, Bias::Right), true);
        });
    }

    /// From the gates, not the handlers; from `TextElement::prepaint` it would land a frame late.
    pub(super) fn reveal_cursor(&mut self, how: Reveal) {
        let viewport = self.scroll_handle.bounds();
        // Bounds stay default() until the first prepaint; a waiting request means a stale layout.
        let geometry = self
            .last_text_top
            .zip(self.last_layout.as_ref())
            .filter(|_| viewport.size.height > px(0.) && self.scroll_to.is_none());
        let Some((text_top, layout)) = geometry else {
            self.request_scroll(ScrollTo::Cursor(how));
            return;
        };

        let row = layout.row_for_offset(self.cursor());
        let top = text_top + layout.top_for_row(row);
        let extent = top..top + layout.row_height(row);
        let viewport = viewport.top()..viewport.bottom();

        if let Some(offset) = how.offset(
            extent,
            viewport,
            self.scroll_handle.offset().y,
            layout.body_row_height(),
        ) {
            self.scroll_handle
                .set_offset(point(self.scroll_handle.offset().x, offset));
        }
    }

    pub(super) fn move_vertically(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        direction: Vertical,
        extend: bool,
    ) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let cursor = self.cursor();
        let row = layout.row_for_offset(cursor);

        // Reused verbatim: re-widens a run of downs after a short row (x is
        // from the row's start, so it means the same on every row).
        let goal_x = match self.state.selection().goal() {
            Goal::Column(x) => px(x),
            Goal::None => layout.x_for_offset(cursor),
        };

        let step = |row: usize| match direction {
            Vertical::Up => row.checked_sub(1),
            Vertical::Down => (row + 1 < layout.row_count()).then_some(row + 1),
        };
        let mut target = step(row);
        while let Some(candidate) = target
            && !layout.holds_caret(candidate)
        {
            target = step(candidate);
        }
        let target = target.map(|row| layout.offset_on_row(row, goal_x));

        if let Some(offset) = target {
            let goal = Goal::Column(f32::from(goal_x));
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

    pub(super) fn move_to_row_edge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edge: RowEdge,
    ) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let row = layout.row_bounds(layout.row_for_offset(self.cursor()));
        let target = match edge {
            RowEdge::Start => *row.start(),
            RowEdge::End => *row.end(),
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, target, false);
        });
    }
}
