//! The views of inactive tabs, kept as anchors so edits elsewhere move them (tabs spec, A2).

use std::collections::HashMap;

use gpui::App;
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use crate::editor::{ParkedView, ViewState};

/// An entry for a tab iff it is inactive and shows a note (I1); no view if its note was being trashed.
#[derive(Default)]
pub(super) struct Parked(HashMap<TabId, (EntryId, Option<ParkedView>)>);

impl Parked {
    /// # Panics
    /// If `tab` is already parked: its anchors would never be forgotten.
    pub(super) fn park(&mut self, tab: TabId, note: EntryId, view: Option<ParkedView>) {
        let previous = self.0.insert(tab, (note, view));
        assert!(previous.is_none(), "tab {tab:?} parked twice");
    }

    /// Resolves and forgets the tab's anchors; `None` if it parked no view.
    pub(super) fn unpark(&mut self, tab: TabId, cx: &mut App) -> Option<ViewState> {
        self.0
            .remove(&tab)
            .and_then(|(_, view)| view)
            .map(|view| view.unpark(cx))
    }

    /// The tab's view where its anchors are now, forgetting none; `None` if it parked no view.
    pub(super) fn peek(&self, tab: TabId, cx: &App) -> Option<ViewState> {
        self.0.get(&tab)?.1.as_ref().map(|view| view.peek(cx))
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
