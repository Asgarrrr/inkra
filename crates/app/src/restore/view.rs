//! A saved view to and from the editor's `ViewState`.

use gpui::px;
use inkra_editor::{Goal, Selection};
use inkra_text::ByteOffset;

use super::SavedView;
use crate::editor::{TopLine, ViewState};

impl From<&ViewState> for SavedView {
    fn from(view: &ViewState) -> Self {
        Self {
            tail: view.selection.tail().0,
            head: view.selection.head().0,
            top: view.top.offset().0,
            below: view.top.below().into(),
        }
    }
}

/// Unclipped: the note may have changed since, so the binding clips it.
impl From<SavedView> for ViewState {
    fn from(saved: SavedView) -> Self {
        let mut selection = Selection::at(ByteOffset(saved.tail));
        selection.set_head(ByteOffset(saved.head), Goal::None);
        Self {
            selection,
            top: TopLine::new(ByteOffset(saved.top), px(saved.below)),
        }
    }
}

#[cfg(test)]
mod tests;
