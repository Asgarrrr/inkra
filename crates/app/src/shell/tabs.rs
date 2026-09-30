//! Tabs over the one editor: a switch captures the view left and binds the place arriving (tabs spec, A3).

use gpui::{App, Context, Window, actions};
use inkra_tabs::{Closed, Direction, TabId};
use inkra_vault::EntryId;

use super::parked::Parked;
use super::{AppShell, Bound};
use crate::editor::ViewState;
use crate::vault::NoteStore;

actions!(
    tabs,
    [NewTab, CloseTab, NextTab, PreviousTab, ActivateLastTab]
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

/// The place being left; `Tabs` calls this only when the active tab really is left.
pub(super) fn capture(bound: Option<&Bound>, cx: &mut App) -> Place {
    bound.map_or_else(Place::default, |bound| Place {
        note: Some(bound.note),
        view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
    })
}

/// As `capture`, for a tab left inactive: its view goes to `parked`, never into `Tabs` (I2).
fn park(parked: &mut Parked, tab: TabId, bound: Option<&Bound>, cx: &mut App) -> Place {
    let Some(bound) = bound else {
        return Place::default();
    };
    let view = bound.editor.update(cx, |editor, cx| editor.park(cx));
    parked.park(tab, bound.note, view);
    Place {
        note: Some(bound.note),
        view: None,
    }
}

impl AppShell {
    pub(super) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let (at, left) = (self.tabs.active_index() + 1, self.tabs.active());
        self.tabs.insert(at, Place::default(), || {
            park(&mut self.parked, left, self.bound.as_ref(), cx)
        });
        self.show_place(Place::default(), window, cx);
        self.keep_active_tab_visible();
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
        let closed = self.tabs.close(id, || capture(self.bound.as_ref(), cx));
        // Finds an entry only for an inactive tab; S5 keeps its view for the closed stack.
        let _ = self.parked.unpark(id, cx);
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
            .activate(id, || park(&mut self.parked, left, self.bound.as_ref(), cx));
        if let Some(place) = arriving {
            let place = self.arrive(id, place, cx);
            self.show_place(place, window, cx);
        }
        self.keep_active_tab_visible();
    }

    /// Empties every tab on `id` and drops its parked views; returns whether the active tab was one.
    pub(super) fn forget_note(&mut self, id: EntryId) -> bool {
        self.parked.drop_note(id);
        self.tabs.forget(|place| place.note == Some(id))
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

    /// One update sets the editor, `showing` and `current`, so none is seen without the others.
    fn show_place(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.activation = None;
        let loaded = place
            .note
            .and_then(|id| Some((id, self.notes.read(cx).buffer_for(id)?.clone())));
        if let Some((id, buffer)) = loaded {
            self.bind(id, buffer, place.view, cx);
        } else {
            // Unreachable while nothing evicts a note; forgetting beats a panic.
            if let Some(id) = place.note {
                let _ = self.forget_note(id);
            }
            self.unfocus_editor(window, cx);
            self.set_showing(None, cx);
            self.notes.update(cx, NoteStore::clear_current);
        }
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
}
