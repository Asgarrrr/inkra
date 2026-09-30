//! The views of inactive tabs, kept as anchors so edits elsewhere move them (tabs spec, A2).

use std::collections::HashMap;

use gpui::App;
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use crate::editor::{ParkedView, ViewState};

/// An entry for a tab iff it is inactive and shows a note (I1).
#[derive(Default)]
pub(super) struct Parked(HashMap<TabId, (EntryId, ParkedView)>);

impl Parked {
    /// # Panics
    /// If `tab` is already parked: its anchors would never be forgotten.
    pub(super) fn park(&mut self, tab: TabId, note: EntryId, view: ParkedView) {
        let previous = self.0.insert(tab, (note, view));
        assert!(previous.is_none(), "tab {tab:?} parked twice");
    }

    /// Resolves and forgets the tab's anchors; `None` if it parked nothing.
    pub(super) fn unpark(&mut self, tab: TabId, cx: &mut App) -> Option<ViewState> {
        self.0.remove(&tab).map(|(_, view)| view.unpark(cx))
    }

    /// Drops every entry on `note`, whose buffer closed, without forgetting its anchors.
    pub(super) fn drop_note(&mut self, note: EntryId) {
        self.0.retain(|_, (entry, _)| *entry != note);
    }
}

#[cfg(test)]
impl Parked {
    pub(super) fn tabs(&self) -> std::collections::HashSet<TabId> {
        self.0.keys().copied().collect()
    }
}
