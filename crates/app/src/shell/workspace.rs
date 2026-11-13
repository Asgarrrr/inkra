//! The one writer of `layout`, the lookups it reads, and the invariants checked after each change (splits spec, L1-L7).

use std::collections::HashSet;

use gpui::{App, Context, Window};
use inkra_layout::{GroupId, Side, Surface, ViewId};
use inkra_tabs::TabId;

use super::AppShell;
use crate::pane::PaneEntry;

/// Each group in `Layout::groups` order: its views in tab order, its active view and its tab count.
type TabState = Vec<(Vec<ViewId>, Option<ViewId>, usize)>;

impl AppShell {
    /// `None` for a view no tab holds.
    pub(super) fn tab_of(&self, view: ViewId) -> Option<(GroupId, TabId)> {
        self.layout.groups().find_map(|group| {
            let (tab, _) = self.group(group).views().find(|&(_, held)| held == view)?;
            Some((group, tab))
        })
    }

    fn tab_state(&self) -> TabState {
        self.layout
            .groups()
            .map(|group| {
                let group = self.group(group);
                let group_views = group.views().map(|(_, view)| view).collect();
                (group_views, group.shown.view(), group.tabs.count())
            })
            .collect()
    }

    /// Each group's active view, where that group is in the centre or a drawn dock.
    pub(super) fn visible_views(&self) -> HashSet<ViewId> {
        self.layout
            .groups()
            .filter(|&group| match self.layout.surface_of(group) {
                Surface::Center => true,
                Surface::Dock(side) => self.is_dock_drawn(side),
            })
            .filter_map(|group| self.group(group).shown.view())
            .collect()
    }

    /// The one writer of `layout` and the dock groups. A pane that stops showing hears `hidden` and gives
    /// up focus, so no key lands out of sight; one that starts showing hears `shown`, or its dormant
    /// feature activates. An open menu closes: it describes the layout it opened on.
    pub(super) fn change_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) {
        if self.modal_menu().is_some() {
            self.dismiss_modal(window, cx);
        }
        if self.panes.is_none() {
            return;
        }
        let (layout, tabs) = (self.layout.clone(), self.tab_state());
        let visible = self.visible_views();
        change(self, window, cx);
        if self.layout == layout && self.tab_state() == tabs {
            return;
        }
        let now = self.visible_views();
        for &view in visible.difference(&now) {
            self.pane_hidden(view, window, cx);
        }
        for &view in now.difference(&visible) {
            self.pane_shown(view, window, cx);
        }
        if cfg!(debug_assertions) {
            self.check_layout(cx);
        }
        cx.notify();
        self.save_session_later(cx);
    }

    /// `None` with no vault, or for a pane its feature has not registered.
    pub(super) fn pane_entry(&self, view: ViewId) -> Option<&PaneEntry> {
        self.panes.as_ref()?.entries.get(&view)
    }

    fn pane_hidden(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.pane_entry(view) else {
            return;
        };
        let focused = entry
            .focus(cx)
            .is_some_and(|focus| focus.contains_focused(window, cx));
        entry.hidden(cx);
        if focused {
            self.land_focus(window, cx);
        }
    }

    fn pane_shown(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        match self.pane_entry(view) {
            Some(entry) => entry.shown(cx),
            None => self.wake_pane(view, window, cx),
        }
    }

    /// Debug builds check these after every layout change and the launch's.
    ///
    /// # Panics
    /// On a broken one.
    pub(super) fn check_layout(&self, cx: &App) {
        let held: HashSet<GroupId> = self.layout.groups().collect();
        assert_eq!(
            self.groups.keys().copied().collect::<HashSet<_>>(),
            held,
            "a group for each of the layout's, and no other"
        );
        let last = self.layout.last_center();
        assert!(
            self.layout.center_groups().contains(&last),
            "the last active group is in the centre"
        );
        for group in self.layout.center_groups() {
            assert!(
                self.editor_of(group)
                    .is_none_or(|editor| editor.read(cx).is_pinned() == (group != last)),
                "an editor is pinned exactly when its group is not the last active"
            );
        }
        for side in [Side::Left, Side::Right] {
            for group in self.layout.dock_groups(side) {
                let group = self.group(group);
                assert_eq!(
                    group.views().count(),
                    group.tabs.count(),
                    "a dock tab holds a view"
                );
            }
        }
        let views: Vec<ViewId> = held
            .iter()
            .flat_map(|&group| self.group(group).views().map(|(_, view)| view))
            .collect();
        for view in self.pane_decls().map(|pane| pane.id) {
            let tabs = views.iter().filter(|&&held| held == view).count();
            assert_eq!(tabs, 1, "{view:?} sits in exactly one tab");
        }
    }
}
