//! One tab group: its tabs, what its active tab shows and its pending open, and the group's lifecycle (splits spec, App).

use std::fmt::Display;

use gpui::{Context, ScrollHandle, Task, Window};
use inkra_layout::{GroupId, SplitDirection, Surface, View};
use inkra_tabs::{TabId, Tabs};

use super::parked::Parked;
use super::tabs::{Place, capture};
use super::{AppShell, Shown};
use crate::editor::EditorView;

/// The debug selector `name` takes in the centre's group `k`: bare in the first, so one group keeps every name.
pub(super) fn selector(name: impl Display, k: usize) -> String {
    if k == 0 {
        name.to_string()
    } else {
        format!("{name}-{k}")
    }
}

pub(super) struct Group {
    pub(super) tabs: Tabs<Place, Parked>,
    /// Written only by `set_shown`. Not `NoteStore::current`, which `trash` clears first.
    pub(super) shown: Shown,
    /// The bar's horizontal scroll, moved to the active tab on every switch.
    pub(super) tab_scroll: ScrollHandle,
    /// Held, never detached: dropping a `Task` cancels it, so a superseded
    /// load cannot rebind the editor behind a later activation.
    pub(super) activation: Option<Task<()>>,
    /// The edge a drop held over the body would split towards; written on each drag move.
    pub(super) drop_split: Option<SplitDirection>,
}

impl Group {
    /// One empty tab.
    pub(super) fn new() -> Self {
        Self::with(Tabs::new(), Shown::Empty)
    }

    /// A tab on each of `views`, `active` the one shown.
    ///
    /// # Panics
    /// As `Tabs::restored`: on no views, or `active` out of range.
    pub(super) fn holding(views: &[View], active: usize) -> Self {
        let tabs = Tabs::restored(views.iter().copied().map(Parked::View).collect(), active);
        Self::with(tabs, Shown::View(views[active]))
    }

    /// One empty tab, no open pending, nothing to reopen.
    pub(super) fn holds_nothing(&self) -> bool {
        self.tabs.count() == 1
            && matches!(self.shown, Shown::Empty)
            && self.activation.is_none()
            && !self.tabs.has_closed()
    }

    pub(super) fn with(tabs: Tabs<Place, Parked>, shown: Shown) -> Self {
        Self {
            tabs,
            shown,
            tab_scroll: ScrollHandle::new(),
            activation: None,
            drop_split: None,
        }
    }

    /// The view each tab holds, in tab order; a tab holding none is skipped.
    pub(super) fn views(&self) -> impl Iterator<Item = (TabId, View)> + '_ {
        self.tabs.iter().filter_map(|tab| {
            let view = tab
                .place()
                .map_or_else(|| self.shown.view(), Parked::view)?;
            Some((tab.id(), view))
        })
    }
}

impl AppShell {
    /// Focus in the group left moves to the one entered: a pinned editor takes no keys.
    pub(super) fn activate_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let left = self.layout.last_center();
        if group == left {
            return;
        }
        let refocus = self.editor_of(left).is_some_and(|editor| {
            gpui::Focusable::focus_handle(editor, cx).contains_focused(window, cx)
        });
        self.change_layout(window, cx, |shell, _, cx| shell.switch_group(group, cx));
        if refocus && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// Closes `group`'s one tab `id` and removes the group; the heir takes its room and its closed
    /// stack, so `⌘⇧T` brings the tab back there (L6). Its pending open drops with it (T4).
    pub(super) fn close_group(
        &mut self,
        group: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert_eq!(self.layout.surface_of(group), Surface::Center);
        let was_last = group == self.layout.last_center();
        self.change_layout(window, cx, |shell, window, cx| {
            let Group { tabs, shown, .. } = shell.group_mut(group);
            tabs.close(id, |_| capture(shown, cx));
            shell.remove_group(group, window, cx);
        });
        if was_last && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// Takes `group` out of the layout; its heir takes its room, its closed stack and, if it was
    /// `last_center`, that place (L6). A dock's only group leaves no heir.
    pub(super) fn remove_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Group {
        if group == self.layout.last_center() {
            self.unfocus_editor(group, window, cx);
        }
        self.detach_group(group, cx)
    }

    /// `remove_group` for a group whose editor, if any, does not hold the focus.
    pub(super) fn detach_group(&mut self, group: GroupId, cx: &mut Context<Self>) -> Group {
        let was_last = group == self.layout.last_center();
        if !was_last && let Some(editor) = self.editor_of(group) {
            editor.update(cx, EditorView::unpin);
        }
        let heir = self.layout.remove(group);
        let Some(mut removed) = self.groups.remove(&group) else {
            panic!("{group:?} has no group");
        };
        if let Some(heir) = heir {
            self.group_mut(heir)
                .tabs
                .extend_closed(removed.tabs.take_closed());
            if was_last {
                self.enter_group(heir, cx);
            }
        }
        Self::release_images_later(cx);
        removed
    }

    pub(super) fn switch_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor() {
            editor.update(cx, EditorView::pin);
        }
        self.enter_group(group, cx);
    }

    /// `group` becomes `last_center`, unpinned, and what it shows reaches the note's followers.
    pub(super) fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_of(group) {
            editor.update(cx, EditorView::unpin);
        }
        self.layout.activate(group);
        let since = self.tree_cursor(cx);
        self.follow_center(since, cx);
        self.keep_active_tab_visible(group);
    }
}
