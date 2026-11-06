//! Tabs over the one editor: a switch captures the view left and binds the place arriving (tabs spec, A3).

use std::ops::Range;

use gpui::{App, Context, Window, actions};
use inkra_layout::{GroupId, View};
use inkra_tabs::{Closed, Direction, TabId};
use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use super::group::Group;
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

/// A tab on a note; `view: None` opens it at the top.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OnNote<V> {
    pub(super) note: EntryId,
    pub(super) view: Option<V>,
}

/// What a tab shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) enum Place {
    #[default]
    Empty,
    Note(OnNote<ViewState>),
    View(View),
}

impl inkra_tabs::Place for Place {
    type Note = EntryId;

    fn note(&self) -> Option<EntryId> {
        match self {
            Self::Note(on) => Some(on.note),
            Self::Empty | Self::View(_) => None,
        }
    }
}

/// The place being left; `Tabs` calls this only when the active tab really is left.
pub(super) fn capture(shown: &Shown, cx: &mut App) -> Place {
    match shown {
        Shown::Empty => Place::Empty,
        Shown::View(view) => Place::View(*view),
        Shown::Trashing(note) => Place::Note(OnNote {
            note: *note,
            view: None,
        }),
        Shown::Bound(bound) => Place::Note(OnNote {
            note: bound.note,
            view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
        }),
    }
}

impl AppShell {
    pub(super) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let center = self.layout.center();
        let Group { tabs, shown, .. } = self.group_mut(center);
        tabs.insert(tabs.active_index() + 1, || Parked::park(shown, cx));
        self.show_place(center, Place::Empty, window, cx);
        self.keep_active_tab_visible(center);
    }

    /// Inserted when the note lands: an empty tab first would drop the editor and let a second open cancel this one.
    pub(super) fn open_in_new_tab(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let center = self.layout.center();
        let (source, since) = (self.group(center).tabs.active(), self.tree_cursor(cx));
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.group_mut(center).activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                let Group { tabs, shown, .. } = shell.group_mut(center);
                // T4: `show` drops `activation` on every other change to what the active tab shows.
                if tabs.active() != source {
                    return;
                }
                tabs.insert(tabs.active_index() + 1, || Parked::park(shown, cx));
                shell.land(id, buffer, at, since, window, cx);
                shell.keep_active_tab_visible(center);
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
        self.close_tab(self.center().tabs.active(), window, cx);
    }

    /// Showing the tab now active saves the note left, through `set_current` or `clear_current`.
    pub(super) fn close_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.layout.center();
        let Group { tabs, shown, .. } = self.group_mut(center);
        // Plain offsets: the closed stack keeps no anchors (A2).
        let closed = tabs.close(id, |slot| match slot {
            None => capture(shown, cx),
            Some(parked) => parked.unpark(cx),
        });
        if let Some(Closed {
            shown: Some((_, parked)),
            ..
        }) = closed
        {
            let place = parked.unpark(cx);
            self.show_place(center, place, window, cx);
        } else {
            cx.notify();
        }
        self.keep_active_tab_visible(center);
        self.save_session_later(cx);
    }

    /// Parks the tab left, as `new_tab`; the place arriving is plain, from the closed stack.
    pub(super) fn reopen_closed_tab(
        &mut self,
        _: &ReopenClosedTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        let center = self.layout.center();
        let Group { tabs, shown, .. } = self.group_mut(center);
        let arriving = tabs.reopen(|| Parked::park(shown, cx));
        if let Some(place) = arriving {
            self.show_place(center, place, window, cx);
            self.keep_active_tab_visible(center);
        }
    }

    /// A drop in the bar: nothing is left, captured or parked, so a pending open still lands (T4).
    pub(super) fn move_tab(&mut self, id: TabId, to: usize, cx: &mut Context<Self>) {
        let center = self.layout.center();
        self.group_mut(center).tabs.move_tab(id, to);
        self.keep_active_tab_visible(center);
        self.save_session_later(cx);
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
        if let Some(id) = self.center().tabs.id_at(action.0) {
            self.activate_tab(self.layout.center(), id, window, cx);
        }
    }

    pub(super) fn activate_last_tab(
        &mut self,
        _: &ActivateLastTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        self.activate_tab(self.layout.center(), self.center().tabs.last(), window, cx);
    }

    fn step_tab(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let id = self.center().tabs.step(direction);
        self.activate_tab(self.layout.center(), id, window, cx);
    }

    pub(super) fn activate_tab(
        &mut self,
        group: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Group { tabs, shown, .. } = self.group_mut(group);
        let arriving = tabs.activate(id, || Parked::park(shown, cx));
        if let Some(parked) = arriving {
            let place = parked.unpark(cx);
            self.show_place(group, place, window, cx);
        }
        self.keep_active_tab_visible(group);
    }

    pub(super) fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let (center, note) = (self.layout.center(), self.active_note());
        let Group { tabs, shown, .. } = self.group_mut(center);
        // Captured, not parked: the tab stays active, and the stack keeps plain offsets (A2).
        let arriving = tabs.back(note, || capture(shown, cx));
        if let Some(place) = arriving {
            self.show_place(center, place, window, cx);
        }
    }

    pub(super) fn go_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        let (center, note) = (self.layout.center(), self.active_note());
        let Group { tabs, shown, .. } = self.group_mut(center);
        let arriving = tabs.forward(note, || capture(shown, cx));
        if let Some(place) = arriving {
            self.show_place(center, place, window, cx);
        }
    }

    /// Drops the view with the note: a buffer with no file is still typable, and every keystroke would be lost.
    pub(super) fn note_closed(&mut self, id: EntryId, window: &mut Window, cx: &mut Context<Self>) {
        // No focus move: a trash from the tree keeps the focus there.
        if self.forget_note(id) {
            self.display(self.layout.center(), Place::Empty, window, cx);
        }
        self.save_session_later(cx);
        cx.notify();
    }

    /// Binds `id` again if the active tab waited on its trash.
    pub(super) fn trash_failed(
        &mut self,
        id: EntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Shown::Trashing(note) = self.center().shown
            && note == id
        {
            let place = Place::Note(OnNote {
                note: id,
                view: None,
            });
            self.display(self.layout.center(), place, window, cx);
            self.focus_note_from_root(window, cx);
            cx.notify();
        }
    }

    /// Empties every tab on `id`, dropping its parked views; returns whether the active tab was one.
    pub(super) fn forget_note(&mut self, id: EntryId) -> bool {
        let active = self.active_note() == Some(id);
        self.center_mut().tabs.forget(id);
        active
    }

    /// After every change to the tab set or the active tab: a close to its left shifts it too.
    pub(super) fn keep_active_tab_visible(&self, group: GroupId) {
        let group = self.group(group);
        group.tab_scroll.scroll_to_item(group.tabs.active_index());
    }

    /// As `show`, then focus lands on the note; a dock's switch leaves it where it is.
    fn show_place(
        &mut self,
        group: GroupId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show(group, place, window, cx);
        if group == self.layout.center()
            && let Some(focus) = self.land_focus(cx)
        {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// A user action on what the active tab shows: it cancels a pending open (T4), then `display`s.
    pub(super) fn show(
        &mut self,
        group: GroupId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.group_mut(group).activation = None;
        self.display(group, place, window, cx);
    }

    /// A trash outcome's change, as a landing's: it keeps a pending open (T4).
    /// One update sets the editor, `showing` and `current`, so none is seen without the others.
    /// A view binds no editor and leaves `current` alone.
    pub(super) fn display(
        &mut self,
        group: GroupId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let since = self.tree_cursor(cx);
        let shown = match place {
            Place::Empty => Shown::Empty,
            Place::View(view) => Shown::View(view),
            Place::Note(on) => {
                let buffer = self.notes.read(cx).buffer_for(on.note).cloned();
                if let Some(buffer) = buffer {
                    self.bind(group, on.note, buffer, on.view, since, cx);
                    return;
                }
                // A place's note has no buffer only while its trash is pending; its tabs keep it until `NoteClosed`.
                Shown::Trashing(on.note)
            }
        };
        let clears = group == self.layout.center() && shown.view().is_none();
        self.unfocus_editor(group, window, cx);
        self.set_shown(group, shown, since, cx);
        if clears {
            self.notes.update(cx, NoteStore::clear_current);
        }
    }
}
