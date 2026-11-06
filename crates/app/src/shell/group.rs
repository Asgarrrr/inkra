//! One tab group: its tabs, what its active tab shows and its pending open; the centre's split and activation (splits spec, App).

use std::fmt::Display;

use gpui::{Context, ScrollHandle, Task, Window, actions};
use inkra_layout::{Axis, GroupId, View};
use inkra_tabs::{TabId, Tabs};

use super::parked::Parked;
use super::tabs::{OnNote, Place, capture};
use super::{AppShell, Shown};
use crate::editor::EditorView;

actions!(pane, [SplitRight, SplitDown]);

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

    fn with(tabs: Tabs<Place, Parked>, shown: Shown) -> Self {
        Self {
            tabs,
            shown,
            tab_scroll: ScrollHandle::new(),
            activation: None,
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
    pub(super) fn split_right(
        &mut self,
        _: &SplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split(Axis::Horizontal, window, cx);
    }

    pub(super) fn split_down(
        &mut self,
        _: &SplitDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split(Axis::Vertical, window, cx);
    }

    /// The new group shows the note with the caret it had, and takes the focus.
    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, window, cx| {
            let from = shell.layout.last_center();
            let on = shell.group(from).shown.bound().map(|bound| OnNote {
                note: bound.note,
                view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
            });
            let split = shell.layout.split(from, axis);
            shell.groups.insert(split, Group::new());
            if let Some(on) = on {
                shell.display(split, Place::Note(on), window, cx);
            }
            shell.switch_group(split, cx);
        });
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

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
        let was_last = group == self.layout.last_center();
        self.change_layout(window, cx, |shell, window, cx| {
            let Group { tabs, shown, .. } = shell.group_mut(group);
            tabs.close(id, |_| capture(shown, cx));
            let closed = tabs.take_closed();
            if was_last {
                shell.unfocus_editor(group, window, cx);
            } else if let Some(editor) = shell.editor_of(group) {
                editor.update(cx, EditorView::unpin);
            }
            let Some(heir) = shell.layout.remove(group) else {
                panic!("a centre group has an heir");
            };
            shell.groups.remove(&group);
            shell.group_mut(heir).tabs.extend_closed(closed);
            if was_last {
                shell.enter_group(heir, cx);
            }
        });
        if was_last && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
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
