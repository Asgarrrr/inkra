//! Tabs over the one editor: a switch captures the view left and binds the place arriving (tabs spec, A3).

use gpui::{App, Context, Window, actions};
use inkra_tabs::{Closed, Direction, TabId};
use inkra_vault::EntryId;

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

impl AppShell {
    pub(super) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let at = self.tabs.active_index() + 1;
        self.tabs
            .insert(at, Place::default(), || capture(self.bound.as_ref(), cx));
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
        if let Some(Closed {
            shown: Some((_, place)),
            ..
        }) = self.tabs.close(id, || capture(self.bound.as_ref(), cx))
        {
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
        if let Some(place) = self.tabs.activate(id, || capture(self.bound.as_ref(), cx)) {
            self.show_place(place, window, cx);
        }
        self.keep_active_tab_visible();
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
                let _ = self.tabs.forget(|place| place.note == Some(id));
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
