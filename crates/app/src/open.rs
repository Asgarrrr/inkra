//! Where an opener shows its file: the active tab, a new one (tabs spec, A5) or a split (splits spec, P5).

use gpui::Modifiers;
use inkra_vault::EntryId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Open {
    Here,
    NewTab,
    SplitRight,
}

impl Open {
    pub(crate) fn from_click(modifiers: Modifiers) -> Self {
        if modifiers.secondary() {
            Self::NewTab
        } else {
            Self::Here
        }
    }
}

/// A struct, not an enum: `EventEmitter<E>` needs one type per event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileActivated {
    pub(crate) file: EntryId,
    pub(crate) open: Open,
}
