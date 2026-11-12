//! Tabs over the one editor: a switch captures the view left and binds the place arriving (tabs spec, A3).

use std::ops::Range;

use gpui::{App, Context, Window, actions};
use inkra_layout::{GroupId, Surface, View};
use inkra_tabs::{Closed, Direction, TabId, Way};
use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use super::activation::Origin;
use super::group::Group;
use super::nav::Walk;
use super::parked::Parked;
use super::{AppShell, Shown};
use crate::editor::ViewState;
use crate::file_kind::FileKind;

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

/// A tab on a file; `view: None` opens it at the top.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OnFile<V> {
    pub(super) file: EntryId,
    pub(super) view: Option<V>,
}

/// What a tab shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) enum Place {
    #[default]
    Empty,
    File(OnFile<ViewState>),
    View(View),
}

impl inkra_tabs::Place for Place {
    type File = EntryId;

    fn file(&self) -> Option<EntryId> {
        match self {
            Self::File(on) => Some(on.file),
            Self::Empty | Self::View(_) => None,
        }
    }
}

/// The place being left; `Tabs` calls this only when the active tab really is left.
pub(super) fn capture(shown: &Shown, cx: &mut App) -> Place {
    match shown {
        Shown::Empty => Place::Empty,
        Shown::View(view) => Place::View(*view),
        Shown::Image(file) | Shown::Trashing(file) => Place::File(OnFile {
            file: *file,
            view: None,
        }),
        Shown::Bound(bound) => Place::File(OnFile {
            file: bound.note,
            view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
        }),
    }
}

impl AppShell {
    pub(super) fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let center = self.layout.last_center();
        let Group { tabs, shown, .. } = self.group_mut(center);
        tabs.insert(tabs.active_index() + 1, || Parked::park(shown, cx));
        self.show_place(center, Place::Empty, window, cx);
        self.keep_active_tab_visible(center);
    }

    /// Inserted when the note lands: an empty tab first would drop the editor and let a second open cancel this one.
    pub(super) fn open_in_new_tab(
        &mut self,
        id: EntryId,
        kind: FileKind,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let origin = Origin {
            group: self.layout.last_center(),
            since: self.tree_cursor(cx),
        };
        let source = self.group(origin.group).tabs.active();
        let opening = self.opening(id, kind, cx);
        self.group_mut(origin.group).activation = Some(cx.spawn(async move |shell, cx| {
            let Some(landing) = opening.landed().await else {
                return;
            };
            let _ = shell.update_in(cx, |shell, window, cx| {
                let Group { tabs, shown, .. } = shell.group_mut(origin.group);
                // T4: `show` drops `activation` on every other change to what the active tab shows.
                if tabs.active() != source {
                    return;
                }
                tabs.insert(tabs.active_index() + 1, || Parked::park(shown, cx));
                shell.land(origin, id, landing, at, window, cx);
                shell.keep_active_tab_visible(origin.group);
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
        self.close_tab(
            self.layout.last_center(),
            self.center().tabs.active(),
            window,
            cx,
        );
    }

    /// Showing the tab now active saves the note left, through `set_current` or `clear_current`.
    /// A split's only tab takes its group with it (L6); the centre's last refills with an empty tab (L3).
    /// A view is never closed: its tab moves to its default side's dock.
    pub(super) fn close_tab(
        &mut self,
        group: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = self.group(group).views().find(|&(tab, _)| tab == id);
        if let Some((_, view)) = view {
            self.append(view, view.default_side(), window, cx);
            return;
        }
        let tabs = &self.group(group).tabs;
        if tabs.count() == 1
            && tabs.active() == id
            && self.layout.surface_of(group) == Surface::Center
            && self.layout.removable(group)
        {
            self.close_group(group, id, window, cx);
            return;
        }
        let Group { tabs, shown, .. } = self.group_mut(group);
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
            self.show_place(group, place, window, cx);
        } else {
            cx.notify();
        }
        self.keep_active_tab_visible(group);
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
        let center = self.layout.last_center();
        let Group { tabs, shown, .. } = self.group_mut(center);
        let arriving = tabs.reopen(|| Parked::park(shown, cx));
        if let Some(place) = arriving {
            self.show_place(center, place, window, cx);
            self.keep_active_tab_visible(center);
        }
    }

    /// A drop in the bar: nothing is left, captured or parked, so a pending open still lands (T4).
    pub(super) fn move_tab(
        &mut self,
        group: GroupId,
        id: TabId,
        to: usize,
        cx: &mut Context<Self>,
    ) {
        self.group_mut(group).tabs.move_tab(id, to);
        self.keep_active_tab_visible(group);
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
            self.activate_tab(self.layout.last_center(), id, window, cx);
        }
    }

    pub(super) fn activate_last_tab(
        &mut self,
        _: &ActivateLastTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        self.activate_tab(
            self.layout.last_center(),
            self.center().tabs.last(),
            window,
            cx,
        );
    }

    fn step_tab(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_modal(window, cx);
        let id = self.center().tabs.step(direction);
        self.activate_tab(self.layout.last_center(), id, window, cx);
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
        self.walk_history(self.layout.last_center(), Walk::Step(Way::Back), window, cx);
    }

    pub(super) fn go_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.walk_history(
            self.layout.last_center(),
            Walk::Step(Way::Forward),
            window,
            cx,
        );
    }

    /// After every change to the tab set or the active tab: a close to its left shifts it too.
    pub(super) fn keep_active_tab_visible(&self, group: GroupId) {
        let group = self.group(group);
        group.tab_scroll.scroll_to_item(group.tabs.active_index());
    }

    /// As `show`, then focus lands on the note; a dock's switch leaves it where it is.
    pub(super) fn show_place(
        &mut self,
        group: GroupId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show(group, place, window, cx);
        if group == self.layout.last_center() {
            self.land_focus(window, cx);
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
            Place::File(on) => {
                self.display_file(group, on, since, window, cx);
                return;
            }
        };
        self.unfocus_editor(group, window, cx);
        self.set_shown(group, shown, since, cx);
    }
}
