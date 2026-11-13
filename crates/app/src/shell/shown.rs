use gpui::Entity;
use inkra_layout::ViewId;
use inkra_vault::EntryId;

use crate::editor::EditorView;

/// The editor and the note it shows, in one field: an editor never shows no note (T2).
pub(super) struct Bound {
    pub(super) note: EntryId,
    pub(super) editor: Entity<EditorView>,
}

/// What the active tab shows; only `Bound` has an editor (T2).
pub(super) enum Shown {
    Empty,
    Bound(Bound),
    Image(EntryId),
    /// The note's trash is pending: no editor until `NoteClosed` or `TrashFailed`.
    /// Entered only from a bound note, so a failed trash always finds its buffer again.
    Trashing(EntryId),
    View(ViewId),
}

impl Shown {
    pub(super) fn bound(&self) -> Option<&Bound> {
        match self {
            Self::Bound(bound) => Some(bound),
            Self::Empty | Self::Image(_) | Self::Trashing(_) | Self::View(_) => None,
        }
    }

    /// The file the tab is on: a note bound or being trashed, or an image.
    pub(super) fn file(&self) -> Option<EntryId> {
        match self {
            Self::Empty | Self::View(_) => None,
            Self::Bound(Bound { note: file, .. }) | Self::Image(file) | Self::Trashing(file) => {
                Some(*file)
            }
        }
    }

    pub(super) fn view(&self) -> Option<ViewId> {
        match self {
            Self::View(view) => Some(*view),
            Self::Empty | Self::Bound(_) | Self::Image(_) | Self::Trashing(_) => None,
        }
    }
}
