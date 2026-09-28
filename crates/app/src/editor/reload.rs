//! A change from disk: the caret, the composition and the top line follow their text (F8).

use gpui::{Context, Entity, Pixels, Window, point, px};
use inkra_text::{Bias, Buffer, ByteOffset, Diff};

use super::EditorView;
use crate::buffer_events::Reloaded;

/// The text at the viewport's top, to put back there once the new text is laid out.
pub(super) struct TopLine {
    /// The start of the top row.
    offset: ByteOffset,
    /// How far the viewport's top sits below that row's top.
    below: Pixels,
}

impl TopLine {
    fn mapped(self, diff: &Diff) -> Self {
        Self {
            offset: diff.map(self.offset, Bias::Left),
            ..self
        }
    }
}

impl EditorView {
    /// Runs before `on_buffer_changed`: `Reloaded` is emitted before the notify.
    pub(super) fn on_buffer_reloaded(
        &mut self,
        _: Entity<Buffer>,
        event: &Reloaded,
        cx: &mut Context<Self>,
    ) {
        let diff = &event.0;
        self.state.map_through(diff);
        // A pending one is in the previous reload's text, which `last_layout` does not describe.
        self.restore_top = self
            .restore_top
            .take()
            .or_else(|| self.top_line())
            .map(|top| top.mapped(diff));
        cx.notify();
    }

    /// The top line in the text the last layout describes; `None` unscrolled.
    fn top_line(&self) -> Option<TopLine> {
        let offset = self.scroll_handle.offset().y;
        if offset == px(0.) {
            return None;
        }
        let layout = self.last_layout.as_ref()?;
        let y = self.viewport_top()? - offset;
        let row = layout.row_for_offset(layout.offset_for_position(point(px(0.), y)));
        Some(TopLine {
            offset: *layout.row_bounds(row).start(),
            below: y - layout.top_for_row(row),
        })
    }

    /// The viewport's top in the text's own y at zero scroll; `None` before the first paint.
    fn viewport_top(&self) -> Option<Pixels> {
        Some(self.scroll_handle.bounds().top() - self.last_text_top?)
    }

    /// From `render`, before prepaint reads the offset, else the new text lands a frame late.
    pub(super) fn restore_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(top) = self.restore_top.take() else {
            return;
        };
        self.reshape(window, cx);
        let (Some(layout), Some(viewport_top)) = (self.last_layout.as_ref(), self.viewport_top())
        else {
            return;
        };
        let row_top = layout.top_for_row(layout.row_for_offset(top.offset));
        let y = viewport_top - (row_top + top.below);
        self.scroll_handle
            .set_offset(point(self.scroll_handle.offset().x, y.min(px(0.))));
    }
}

#[cfg(test)]
impl EditorView {
    /// Scrolls so the row holding `offset` starts at the viewport's top; takes effect at the next draw.
    pub(crate) fn scroll_to_top(&mut self, offset: ByteOffset, cx: &mut Context<Self>) {
        let (Some(layout), Some(viewport_top)) = (self.last_layout.as_ref(), self.viewport_top())
        else {
            panic!("the editor has not been drawn");
        };
        let row_top = layout.top_for_row(layout.row_for_offset(offset));
        let y = viewport_top - row_top;
        self.scroll_handle.set_offset(point(px(0.), y));
        cx.notify();
    }

    /// The offset at the viewport's top-left, read from the last layout.
    pub(crate) fn offset_at_top(&self) -> ByteOffset {
        let (Some(layout), Some(viewport_top)) = (self.last_layout.as_ref(), self.viewport_top())
        else {
            panic!("the editor has not been drawn");
        };
        let y = viewport_top - self.scroll_handle.offset().y;
        layout.offset_for_position(point(px(0.), y))
    }

    pub(crate) fn scroll_y(&self) -> Pixels {
        self.scroll_handle.offset().y
    }
}
