//! Tabs over the one editor: a switch captures the view left and binds the place arriving (tabs spec, A3).

use std::ops::Range;

use gpui::{App, Context, Window, actions};
use inkra_tabs::{Closed, Direction, TabId};
use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use super::parked::Parked;
use super::{AppShell, Shown};
use crate::editor::ViewState;
use crate::vault::NoteStore;

actions!(
    tabs,
    [
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        ActivateLastTab,
        GoBack,
        GoForward,
        ReopenClosedTab
    ]
);

/// `⌘1`–`⌘8`; the palette leaves it out, since it cannot be built without its index.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = tabs, no_json)]
pub(crate) struct ActivateTab(pub(crate) usize);

/// What a tab shows; `note: None` is an empty tab.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Place {
    pub(super) note: Option<EntryId>,
    pub(super) view: Option<ViewState>,
}

impl inkra_tabs::Place for Place {
    type Note = EntryId;

    fn note(&self) -> Option<EntryId> {
        self.note
    }
}

/// The place being left; `Tabs` calls this only when the active tab really is left.
pub(super) fn capture(shown: &Shown, cx: &mut App) -> Place {
    Place {
        note: shown.note(),
        view: shown
            .bound()
            .map(|bound| bound.editor.update(cx, |editor, cx| editor.capture(cx))),
    }
}

/// As `capture`, for a tab left inactive: its view goes to `parked`, never into `Tabs` (I2).
fn park(parked: &mut Parked, tab: TabId, shown: &Shown, cx: &mut App) -> Place {
    let Some(note) = shown.note() else {
        return Place::default();
    };
    let view = shown
        .bound()
        .map(|bound| bound.editor.update(cx, |editor, cx| editor.park(cx)));
    parked.park(tab, note, view);
    Place {
        note: Some(note),
        view: None,
    }
}

impl AppShell {
    pub(super) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let (at, left) = (self.tabs.active_index() + 1, self.tabs.active());
        self.tabs
            .insert(at, || park(&mut self.parked, left, &self.shown, cx));
        self.show_place(Place::default(), window, cx);
        self.keep_active_tab_visible();
    }

    /// Inserted when the note lands: an empty tab first would drop the editor and let a second open cancel this one.
    pub(super) fn open_in_new_tab(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let (source, since) = (self.tabs.active(), self.tree_cursor(cx));
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                // T4: `show` drops `activation` on every other change to what the active tab shows.
                if shell.tabs.active() != source {
                    return;
                }
                shell.tabs.insert(shell.tabs.active_index() + 1, || {
                    park(&mut shell.parked, source, &shell.shown, cx)
                });
                shell.land(id, buffer, at, since, window, cx);
                shell.keep_active_tab_visible();
                cx.notify();
            });
        }));
    }

    pub(super) fn close_active_tab(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        self.close_tab(self.tabs.active(), window, cx);
    }

    /// Showing the tab now active saves the note left, through `set_current` or `clear_current`.
    pub(super) fn close_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        // Plain offsets: the closed stack keeps no anchors (A2). `None` for the active tab or no tab (I1).
        let view = self.parked.unpark(id, cx);
        let closed = self.tabs.close(
            id,
            || capture(&self.shown, cx),
            |place| Place { view, ..place },
        );
        if let Some(Closed {
            shown: Some((shown, place)),
            ..
        }) = closed
        {
            let place = self.arrive(shown, place, cx);
            self.show_place(place, window, cx);
        } else {
            cx.notify();
        }
        self.keep_active_tab_visible();
    }

    /// Parks the tab left, as `new_tab`; the place arriving holds a plain view, so no `arrive`.
    pub(super) fn reopen_closed_tab(
        &mut self,
        _: &ReopenClosedTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        let left = self.tabs.active();
        let arriving = self
            .tabs
            .reopen(|| park(&mut self.parked, left, &self.shown, cx));
        if let Some(place) = arriving {
            self.show_place(place, window, cx);
            self.keep_active_tab_visible();
        }
    }

    /// A drop in the bar: nothing is left, captured or parked, so a pending open still lands (T4).
    pub(super) fn move_tab(&mut self, id: TabId, to: usize, cx: &mut Context<Self>) {
        self.tabs.move_tab(id, to);
        self.keep_active_tab_visible();
        cx.notify();
    }

    pub(super) fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(Direction::Next, window, cx);
    }

    pub(super) fn previous_tab(
        &mut self,
        _: &PreviousTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_tab(Direction::Previous, window, cx);
    }

    pub(super) fn activate_nth_tab(
        &mut self,
        action: &ActivateTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        if let Some(id) = self.tabs.id_at(action.0) {
            self.activate_tab(id, window, cx);
        }
    }

    pub(super) fn activate_last_tab(
        &mut self,
        _: &ActivateLastTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        self.activate_tab(self.tabs.last(), window, cx);
    }

    fn step_tab(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let id = self.tabs.step(direction);
        self.activate_tab(id, window, cx);
    }

    pub(super) fn activate_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let left = self.tabs.active();
        let arriving = self
            .tabs
            .activate(id, || park(&mut self.parked, left, &self.shown, cx));
        if let Some(place) = arriving {
            let place = self.arrive(id, place, cx);
            self.show_place(place, window, cx);
        }
        self.keep_active_tab_visible();
    }

    pub(super) fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let note = self.active_note();
        // Captured, not parked: the tab stays active, and the stack keeps plain offsets (A2).
        let arriving = self.tabs.back(note, || capture(&self.shown, cx));
        if let Some(place) = arriving {
            self.show_place(place, window, cx);
        }
    }

    pub(super) fn go_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        let note = self.active_note();
        let arriving = self.tabs.forward(note, || capture(&self.shown, cx));
        if let Some(place) = arriving {
            self.show_place(place, window, cx);
        }
    }

    /// Drops the view with the note: a buffer with no file is still typable, and every keystroke would be lost.
    pub(super) fn note_closed(&mut self, id: EntryId, window: &mut Window, cx: &mut Context<Self>) {
        // No focus move: a trash from the tree keeps the focus there.
        if self.forget_note(id) {
            self.display(Place::default(), window, cx);
        }
        cx.notify();
    }

    /// Binds `id` again if the active tab waited on its trash.
    pub(super) fn trash_failed(
        &mut self,
        id: EntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Shown::Trashing(note) = self.shown
            && note == id
        {
            let place = Place {
                note: Some(id),
                view: None,
            };
            self.display(place, window, cx);
            // The root holds the focus, or a picker returns to it, only if the tab took it there.
            let root = &self.focus_handle;
            let took_root = root.is_focused(window)
                || self
                    .modal
                    .as_ref()
                    .is_some_and(|modal| modal.restores(root));
            if took_root && let Some(focus) = self.land_focus(cx) {
                window.focus(&focus, cx);
            }
            cx.notify();
        }
    }

    /// Empties every tab on `id` and drops its parked views; returns whether the active tab was one.
    pub(super) fn forget_note(&mut self, id: EntryId) -> bool {
        let active = self.active_note() == Some(id);
        self.parked.drop_note(id);
        self.tabs.forget(id);
        active
    }

    /// The place `Tabs` hands back, with the view its tab parked, resolved now.
    fn arrive(&mut self, tab: TabId, mut place: Place, cx: &mut App) -> Place {
        place.view = self.parked.unpark(tab, cx);
        place
    }

    /// After every change to the tab set or the active tab: a close to its left shifts it too.
    fn keep_active_tab_visible(&self) {
        self.tab_scroll.scroll_to_item(self.tabs.active_index());
    }

    /// As `show`, then focus lands on the note.
    fn show_place(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.show(place, window, cx);
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// A user action on what the active tab shows: it cancels a pending open (T4), then `display`s.
    pub(super) fn show(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.activation = None;
        self.display(place, window, cx);
    }

    /// A trash outcome's change, as a landing's: it keeps a pending open (T4).
    /// One update sets the editor, `showing` and `current`, so none is seen without the others.
    fn display(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        let since = self.tree_cursor(cx);
        let loaded = place
            .note
            .and_then(|id| Some((id, self.notes.read(cx).buffer_for(id)?.clone())));
        if let Some((id, buffer)) = loaded {
            self.bind(id, buffer, place.view, since, cx);
        } else {
            // A place's note has no buffer only while its trash is pending; its tabs keep it until `NoteClosed`.
            let shown = place.note.map_or(Shown::Empty, Shown::Trashing);
            self.unfocus_editor(window, cx);
            self.set_shown(shown, since, cx);
            self.notes.update(cx, NoteStore::clear_current);
        }
    }
}
