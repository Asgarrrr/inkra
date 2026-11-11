//! The one sequence that changes what the view shows: another buffer, or a saved view of the bound one.

use gpui::{Context, Pixels};
use inkra_text::ByteOffset;

use super::EditorView;
use super::viewport::{ScrollTo, TopLine, Viewport};
use crate::text_input::TextInput;

impl EditorView {
    /// Leaves the bound buffer's anchors, lets `swap` change the input, then lands on the buffer bound after it.
    pub(super) fn swap_view(
        &mut self,
        top: Option<TopLine>,
        scrolls: &[(ByteOffset, Pixels)],
        swap: impl FnOnce(&mut TextInput),
        cx: &mut Context<Self>,
    ) {
        // Before the swap: the anchors resolve only in the buffer they were set in.
        let pinned = self.drop_pin(cx);
        self.forget_scrolls(cx);
        swap(&mut self.input);
        if pinned {
            self.pinned = Some(self.park_selection(cx));
        }
        self.anchor_scrolls(scrolls, cx);
        self.is_selecting = false;
        self.viewport = Viewport::default();
        // Never a reveal: the restored top line is where the reader left it.
        if let Some(top) = top {
            self.viewport.request(ScrollTo::Top(top));
        }
        cx.notify();
    }
}
