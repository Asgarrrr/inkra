//! The place an inactive tab holds in `Tabs`: its view kept as anchors so edits elsewhere move it (tabs spec, A2).

use gpui::App;
use inkra_layout::View;
use inkra_vault::EntryId;

use super::Shown;
use super::tabs::{OnNote, Place};
use crate::editor::{ParkedView, ViewState};

/// Not `Clone`, and bound only through `unpark`: its anchors resolve once.
#[must_use]
#[derive(Default)]
pub(super) enum Parked {
    #[default]
    Empty,
    Note(OnNote<ParkedView>),
    View(View),
}

impl inkra_tabs::Place for Parked {
    type Note = EntryId;

    fn note(&self) -> Option<EntryId> {
        match self {
            Self::Note(on) => Some(on.note),
            Self::Empty | Self::View(_) => None,
        }
    }
}

impl Parked {
    /// A place on `note`, its view already anchored; `None` if its note was being trashed.
    pub(super) fn new(note: EntryId, view: Option<ParkedView>) -> Self {
        Self::Note(OnNote { note, view })
    }

    /// The active tab's place, its view anchored; seals the undo group (A3).
    pub(super) fn park(shown: &Shown, cx: &mut App) -> Self {
        if let Some(view) = shown.view() {
            return Self::View(view);
        }
        let Some(note) = shown.note() else {
            return Self::Empty;
        };
        let view = shown
            .bound()
            .map(|bound| bound.editor.update(cx, |editor, cx| editor.park(cx)));
        Self::new(note, view)
    }

    /// Resolves and forgets the anchors: the plain place to bind.
    pub(super) fn unpark(self, cx: &mut App) -> Place {
        match self {
            Self::Empty => Place::Empty,
            Self::Note(on) => Place::Note(OnNote {
                note: on.note,
                view: on.view.map(|view| view.unpark(cx)),
            }),
            Self::View(view) => Place::View(view),
        }
    }

    /// The view where its anchors are now, forgetting none (restore-tabs spec, R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState> {
        let Self::Note(on) = self else {
            return None;
        };
        Some(on.view.as_ref()?.peek(cx))
    }

    pub(super) fn view(&self) -> Option<View> {
        match self {
            Self::View(view) => Some(*view),
            Self::Empty | Self::Note(_) => None,
        }
    }
}
