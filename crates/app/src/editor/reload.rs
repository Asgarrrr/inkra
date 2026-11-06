//! A change from disk: the caret, the composition and the top line follow their text (F8).

use std::rc::Rc;

use gpui::{Context, Entity};
use inkra_text::Buffer;

use super::EditorView;
use crate::buffer_events::Reloaded;

impl EditorView {
    /// Runs before `on_buffer_changed`: `Reloaded` is emitted before the notify.
    pub(super) fn on_buffer_reloaded(
        &mut self,
        _: Entity<Buffer>,
        event: &Reloaded,
        cx: &mut Context<Self>,
    ) {
        let diff = &event.0;
        // A pin's anchors already moved with the text.
        if self.pinned.is_none() {
            self.input.map_through(diff);
        }
        // The layout before any reshape: the top line is read in the old text.
        let layout = self.input.last_layout().map(Rc::as_ref);
        self.viewport.reloaded(diff, layout);
        cx.notify();
    }
}
