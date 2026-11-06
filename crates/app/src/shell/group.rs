//! One tab group: its tabs, what its active tab shows and its pending open; the centre's split and activation (splits spec, App).

use std::fmt::Display;

use gpui::{Context, ScrollHandle, Task, Window, actions};
use inkra_layout::{GroupId, View};
use inkra_tabs::{TabId, Tabs};

use super::parked::Parked;
use super::tabs::{OnNote, Place};
use super::{AppShell, Shown};
use crate::editor::EditorView;

actions!(pane, [SplitRight]);

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
    /// The new group shows the note with the caret it had, and takes the focus.
    pub(super) fn split_right(
        &mut self,
        _: &SplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_layout(window, cx, |shell, window, cx| {
            let from = shell.layout.last_center();
            let on = shell.group(from).shown.bound().map(|bound| OnNote {
                note: bound.note,
                view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
            });
            let split = shell.layout.split_right(from);
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

    fn switch_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor() {
            editor.update(cx, EditorView::pin);
        }
        if let Some(editor) = self.editor_of(group) {
            editor.update(cx, EditorView::unpin);
        }
        self.enter_group(group, cx);
    }

    /// `group` becomes `last_center`, and what it shows reaches the note's followers.
    fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        self.layout.activate(group);
        let since = self.tree_cursor(cx);
        self.follow_center(since, cx);
        self.keep_active_tab_visible(group);
    }
}
