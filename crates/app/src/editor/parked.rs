//! An inactive tab's view kept as anchors in its own buffer, so edits made elsewhere move it (tabs spec, A2).

use gpui::{App, Entity, Pixels};
use inkra_editor::ParkedSelection;
use inkra_text::{Anchor, Bias, Buffer};

use super::viewport::ParkedTop;
use super::{EditorView, ViewState};

/// Owns its buffer and is not `Clone`: its anchors resolve only there, and only once.
pub(crate) struct ParkedView {
    buffer: Entity<Buffer>,
    selection: ParkedSelection,
    top: ParkedTop,
    /// Each block's start biased as `BlockScrolls` biases it.
    scrolls: Vec<(Anchor, Pixels)>,
}

impl ParkedView {
    /// Clips first: a waiting top line may lie past an edit that bypassed the editor, or a saved view past a shorter file.
    pub(crate) fn new(buffer: Entity<Buffer>, view: ViewState, cx: &mut App) -> Self {
        let view = view.clipped(buffer.read(cx));
        // `update`, never `as_mut`: a lease notifies on drop, which schedules an autosave.
        let (selection, top, scrolls) = buffer.update(cx, |buffer, _| {
            (
                ParkedSelection::park(&view.selection, buffer),
                view.top.park(buffer),
                view.scrolls
                    .iter()
                    .map(|&(start, x)| {
                        let start = buffer.clip_grapheme(start, Bias::Left);
                        (buffer.anchor(start, Bias::Right), x)
                    })
                    .collect(),
            )
        });
        Self {
            buffer,
            selection,
            top,
            scrolls,
        }
    }

    /// Resolves and forgets every anchor in one update: nothing edits the buffer in between.
    pub(crate) fn unpark(self, cx: &mut App) -> ViewState {
        let Self {
            buffer,
            selection,
            top,
            scrolls,
        } = self;
        buffer.update(cx, |buffer, _| ViewState {
            selection: selection.unpark(buffer),
            top: top.unpark(buffer),
            scrolls: scrolls
                .into_iter()
                .map(|(anchor, x)| {
                    let start = buffer.resolve(anchor);
                    buffer.forget(anchor);
                    (start, x)
                })
                .collect(),
        })
    }

    /// As `unpark`, forgetting nothing: the view stays parked.
    pub(crate) fn peek(&self, cx: &App) -> ViewState {
        let buffer = self.buffer.read(cx);
        ViewState {
            selection: self.selection.peek(buffer),
            top: self.top.peek(buffer),
            scrolls: self
                .scrolls
                .iter()
                .map(|&(anchor, x)| (buffer.resolve(anchor), x))
                .collect(),
        }
    }
}

impl EditorView {
    /// A capture, sealing the undo group, kept as anchors in the bound buffer.
    pub(crate) fn park(&self, cx: &mut App) -> ParkedView {
        let view = self.capture(cx);
        ParkedView::new(self.input.buffer().clone(), view, cx)
    }
}
