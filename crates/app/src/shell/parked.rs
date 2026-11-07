//! The place an inactive tab holds in `Tabs`: its view kept as anchors so edits elsewhere move it (tabs spec, A2).

use gpui::App;
use inkra_layout::View;
use inkra_vault::EntryId;

use super::Shown;
use super::tabs::{OnFile, Place};
use crate::editor::{ParkedView, ViewState};

/// Not `Clone`, and bound only through `unpark`: its anchors resolve once.
#[must_use]
#[derive(Default)]
pub(super) enum Parked {
    #[default]
    Empty,
    File(OnFile<ParkedView>),
    View(View),
}

impl inkra_tabs::Place for Parked {
    type File = EntryId;

    fn file(&self) -> Option<EntryId> {
        match self {
            Self::File(on) => Some(on.file),
            Self::Empty | Self::View(_) => None,
        }
    }
}

impl Parked {
    /// A place on `file`, its view already anchored; `None` if its note was being trashed, or an image.
    pub(super) fn new(file: EntryId, view: Option<ParkedView>) -> Self {
        Self::File(OnFile { file, view })
    }

    /// The active tab's place, its view anchored; seals the undo group (A3).
    pub(super) fn park(shown: &Shown, cx: &mut App) -> Self {
        if let Some(view) = shown.view() {
            return Self::View(view);
        }
        let Some(file) = shown.file() else {
            return Self::Empty;
        };
        let view = shown
            .bound()
            .map(|bound| bound.editor.update(cx, |editor, cx| editor.park(cx)));
        Self::new(file, view)
    }

    /// Resolves and forgets the anchors: the plain place to bind.
    pub(super) fn unpark(self, cx: &mut App) -> Place {
        match self {
            Self::Empty => Place::Empty,
            Self::File(on) => Place::File(OnFile {
                file: on.file,
                view: on.view.map(|view| view.unpark(cx)),
            }),
            Self::View(view) => Place::View(view),
        }
    }

    /// The view where its anchors are now, forgetting none (restore-tabs spec, R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState> {
        let Self::File(on) = self else {
            return None;
        };
        Some(on.view.as_ref()?.peek(cx))
    }

    pub(super) fn view(&self) -> Option<View> {
        match self {
            Self::View(view) => Some(*view),
            Self::Empty | Self::File(_) => None,
        }
    }
}
