//! The place an inactive tab holds in `Tabs`: its view kept as anchors so edits elsewhere move it (tabs spec, A2).

use gpui::App;
use inkra_vault::EntryId;

use super::Shown;
use super::tabs::{OnNote, Place};
use crate::editor::{ParkedView, ViewState};

/// Not `Clone`, and bound only through `unpark`: its anchors resolve once.
#[must_use]
#[derive(Default)]
pub(super) struct Parked(Option<OnNote<ParkedView>>);

impl inkra_tabs::Place for Parked {
    type Note = EntryId;

    fn note(&self) -> Option<EntryId> {
        self.0.as_ref().map(|on| on.note)
    }
}

impl Parked {
    /// A place on `note`, its view already anchored; `None` if its note was being trashed.
    pub(super) fn new(note: EntryId, view: Option<ParkedView>) -> Self {
        Self(Some(OnNote { note, view }))
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
        Place(self.0.map(|on| OnNote {
            note: on.note,
            view: on.view.map(|view| view.unpark(cx)),
        }))
    }

    /// The view where its anchors are now, forgetting none (restore-tabs spec, R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState> {
        let view = self.0.as_ref()?.view.as_ref()?;
        Some(view.peek(cx))
    }
}
