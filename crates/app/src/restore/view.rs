//! A saved view to and from the editor's `ViewState`.

use gpui::px;
use inkra_editor::{Goal, Selection};
use inkra_text::ByteOffset;

use std::path::PathBuf;

use super::{SavedNoteTab, SavedView};
use crate::editor::{TopLine, ViewState};

impl From<&ViewState> for SavedView {
    fn from(view: &ViewState) -> Self {
        Self {
            tail: view.selection.tail().get(),
            head: view.selection.head().get(),
            top: view.top.offset().get(),
            below: view.top.below().into(),
        }
    }
}

impl SavedNoteTab {
    /// A note with no UTF-8 path saves empty, with no view.
    pub(crate) fn new(path: Option<PathBuf>, view: Option<&ViewState>) -> Self {
        let note = path.and_then(|path| path.to_str().map(str::to_owned));
        let view = view.filter(|_| note.is_some()).map(SavedView::from);
        Self { note, view }
    }
}

/// Unclipped: the note may have changed since, so the binding clips it.
impl From<SavedView> for ViewState {
    fn from(saved: SavedView) -> Self {
        let mut selection = Selection::at(ByteOffset::new(saved.tail));
        selection.set_head(ByteOffset::new(saved.head), Goal::None);
        Self {
            selection,
            top: TopLine::new(ByteOffset::new(saved.top), px(saved.below)),
        }
    }
}

#[cfg(test)]
mod tests;
