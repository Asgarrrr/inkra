//! An inactive tab's view kept as anchors in its own buffer, so edits made elsewhere move it (tabs spec, A2).

use gpui::{App, Entity};
use inkra_editor::ParkedSelection;
use inkra_text::Buffer;

use super::viewport::ParkedTop;
use super::{EditorView, ViewState};

/// Owns its buffer and is not `Clone`: its anchors resolve only there, and only once.
pub(crate) struct ParkedView {
    buffer: Entity<Buffer>,
    selection: ParkedSelection,
    top: ParkedTop,
}

impl ParkedView {
    /// Clips first: a waiting top line may lie past an edit that bypassed the editor.
    fn new(buffer: Entity<Buffer>, view: ViewState, cx: &mut App) -> Self {
        let view = view.clipped(buffer.read(cx));
        // `update`, never `as_mut`: a lease notifies on drop, which schedules an autosave.
        let (selection, top) = buffer.update(cx, |buffer, _| {
            (
                ParkedSelection::park(&view.selection, buffer),
                view.top.park(buffer),
            )
        });
        Self {
            buffer,
            selection,
            top,
        }
    }

    /// Resolves and forgets every anchor in one update: nothing edits the buffer in between.
    pub(crate) fn unpark(self, cx: &mut App) -> ViewState {
        let Self {
            buffer,
            selection,
            top,
        } = self;
        buffer.update(cx, |buffer, _| ViewState {
            selection: selection.unpark(buffer),
            top: top.unpark(buffer),
        })
    }
}

impl EditorView {
    /// A capture, sealing the undo group, kept as anchors in the bound buffer.
    pub(crate) fn park(&self, cx: &mut App) -> ParkedView {
        let view = self.capture(cx);
        ParkedView::new(self.input.buffer().clone(), view, cx)
    }
}
