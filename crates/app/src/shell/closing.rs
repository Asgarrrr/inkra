//! Emptying the tabs on a file that left: a closed note, a gone asset, a failed trash's rebind.

use std::collections::HashSet;

use gpui::{App, Context, Window};
use inkra_layout::GroupId;
use inkra_vault::EntryId;

use super::tabs::{OnFile, Place};
use super::{AppShell, Shown};

impl AppShell {
    /// Drops the view with the note: a buffer with no file is still typable, and every keystroke would be lost.
    pub(super) fn file_closed(&mut self, id: EntryId, window: &mut Window, cx: &mut Context<Self>) {
        // No focus move: a trash from the tree keeps the focus there.
        for group in self.forget_file(id) {
            self.display(group, Place::Empty, window, cx);
        }
        self.save_session_later(cx);
        cx.notify();
    }

    /// Closes every file a tab or history holds that left the vault; a note waits for `NoteClosed` (F9).
    pub(super) fn close_gone_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panes) = &mut self.panes else { return };
        let vault = panes.vault.read(cx);
        // No live id left since the last walk: every file the tabs hold is still there.
        if !vault.has_read_whole() || panes.checked.as_ref() == Some(vault.departures()) {
            return;
        }
        panes.checked = Some(vault.departures().clone());
        let gone: HashSet<EntryId> = self
            .layout
            .center_groups()
            .into_iter()
            .flat_map(|group| {
                let group = self.group(group);
                group.tabs.files().chain(group.shown.file())
            })
            .filter(|&id| self.gone(id, cx))
            .collect();
        for id in gone {
            self.file_closed(id, window, cx);
        }
    }

    /// Left the vault and held by no note session: a note waits for `NoteClosed` (F9).
    pub(super) fn gone(&self, id: EntryId, cx: &App) -> bool {
        let notes = self.notes.read(cx);
        notes
            .vault()
            .is_some_and(|vault| vault.read(cx).entry(id).is_none())
            && !notes.holds(id)
    }

    /// Binds `id` again in each group whose active tab waited on its trash.
    pub(super) fn trash_failed(
        &mut self,
        id: EntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let waiting: Vec<_> = self
            .layout
            .center_groups()
            .into_iter()
            .filter(|&group| matches!(self.group(group).shown, Shown::Trashing(note) if note == id))
            .collect();
        if waiting.is_empty() {
            return;
        }
        for group in waiting {
            let place = Place::File(OnFile {
                file: id,
                view: None,
            });
            self.display(group, place, window, cx);
        }
        self.focus_note_from_root(window, cx);
        cx.notify();
    }

    /// Empties every centre tab on `id`, dropping its parked views; returns the groups whose active tab was one.
    pub(super) fn forget_file(&mut self, id: EntryId) -> Vec<GroupId> {
        let center = self.layout.center_groups();
        center
            .into_iter()
            .filter(|&group| {
                let group = self.group_mut(group);
                group.tabs.forget(id);
                group.shown.file() == Some(id)
            })
            .collect()
    }
}
