//! A pin: the selection of an editor that shares its buffer with another view, held as anchors
//! while the other view edits, so it moves with the text (splits spec, S2).

use gpui::{App, Context};
use inkra_editor::ParkedSelection;

use super::EditorView;

impl EditorView {
    /// Seals the undo group even with no edit, as `capture` does, and parks the selection.
    pub(crate) fn pin(&mut self, cx: &mut Context<Self>) {
        if self.pinned.is_some() {
            return;
        }
        let selection = self.input.state().selection();
        // `update`, never `as_mut`: a lease notifies on drop, which schedules an autosave.
        let pinned = self.input.buffer().update(cx, |buffer, _| {
            buffer.finalize_transaction();
            ParkedSelection::park(selection, buffer)
        });
        self.pinned = Some(pinned);
    }

    pub(crate) fn unpin(&mut self, cx: &mut Context<Self>) {
        self.drop_pin(cx);
    }

    pub(crate) fn is_pinned(&self) -> bool {
        self.pinned.is_some()
    }

    /// Forgets the anchors unread: every buffer version already reset the selection to them.
    pub(super) fn drop_pin(&mut self, cx: &mut App) -> bool {
        let Some(pinned) = self.pinned.take() else {
            return false;
        };
        let _ = self
            .input
            .buffer()
            .update(cx, |buffer, _| pinned.unpark(buffer));
        true
    }

    /// The selection as anchors in the bound buffer, unsealed: the pin it replaces already sealed.
    pub(super) fn park_selection(&self, cx: &mut App) -> ParkedSelection {
        let selection = self.input.state().selection();
        self.input
            .buffer()
            .update(cx, |buffer, _| ParkedSelection::park(selection, buffer))
    }
}
