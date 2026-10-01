//! The place an inactive tab holds in `Tabs`: its view kept as anchors so edits elsewhere move it (tabs spec, A2).

use gpui::App;
use inkra_vault::EntryId;

use super::Shown;
use super::tabs::Place;
use crate::editor::{ParkedView, ViewState};

/// Not `Clone`, and bound only through `unpark`: its anchors resolve once.
#[must_use]
#[derive(Default)]
pub(super) struct Parked {
    note: Option<EntryId>,
    view: Option<ParkedView>,
}

impl inkra_tabs::Place for Parked {
    type Note = EntryId;

    fn note(&self) -> Option<EntryId> {
        self.note
    }
}

impl Parked {
    /// A place on `note`, its view already anchored; `None` if its note was being trashed.
    pub(super) fn new(note: EntryId, view: Option<ParkedView>) -> Self {
        Self {
            note: Some(note),
            view,
        }
    }

    /// The active tab's place, its view anchored; seals the undo group (A3).
    pub(super) fn park(shown: &Shown, cx: &mut App) -> Self {
        let Some(note) = shown.note() else {
            return Self::default();
        };
        let view = shown
            .bound()
            .map(|bound| bound.editor.update(cx, |editor, cx| editor.park(cx)));
        Self::new(note, view)
    }

    /// Resolves and forgets the anchors: the plain place to bind.
    pub(super) fn unpark(self, cx: &mut App) -> Place {
        Place {
            note: self.note,
            view: self.view.map(|view| view.unpark(cx)),
        }
    }

    /// The view where its anchors are now, forgetting none (restore-tabs spec, R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState> {
        self.view.as_ref().map(|view| view.peek(cx))
    }
}
