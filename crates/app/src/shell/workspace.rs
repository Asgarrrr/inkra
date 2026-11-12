//! The one writer of `layout`, the lookups it reads, and the invariants checked after each change (splits spec, L1-L7).

use std::collections::HashSet;

use gpui::{App, Context, Focusable, Window};
use inkra_layout::{GroupId, Side, View};
use inkra_tabs::TabId;

use super::AppShell;
use crate::sidebar::Sidebar;

/// Each group in `Layout::groups` order: its views in tab order, its active view and its tab count.
type TabState = Vec<(Vec<View>, Option<View>, usize)>;

impl AppShell {
    /// # Panics
    /// Only on a broken invariant: each view sits in exactly one tab.
    pub(super) fn tab_of(&self, view: View) -> (GroupId, TabId) {
        self.layout
            .groups()
            .find_map(|group| {
                let (tab, _) = self.group(group).views().find(|&(_, held)| held == view)?;
                Some((group, tab))
            })
            .unwrap_or_else(|| panic!("{view:?} sits in no tab"))
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

    /// The one writer of `layout` and the dock groups. A hidden tree keeps neither focus nor an open name field: either
    /// would take keys out of sight. An open menu closes: it describes the layout it opened on.
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
        let files_shown = self.is_visible(View::Files);
        change(self, window, cx);
        if self.layout == layout && self.tab_state() == tabs {
            return;
        }
        if files_shown
            && !self.is_visible(View::Files)
            && let Some(panes) = &self.panes
        {
            let sidebar = panes.sidebar.clone();
            let focused = sidebar.focus_handle(cx).contains_focused(window, cx);
            sidebar.update(cx, Sidebar::cancel_edit);
            if focused {
                self.land_focus(window, cx);
            }
        }
        if cfg!(debug_assertions) {
            self.check_layout(cx);
        }
        cx.notify();
        self.save_session_later(cx);
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
        let views: Vec<View> = held
            .iter()
            .flat_map(|&group| self.group(group).views().map(|(_, view)| view))
            .collect();
        for view in View::ALL {
            let tabs = views.iter().filter(|&&held| held == view).count();
            assert_eq!(tabs, 1, "{view:?} sits in exactly one tab");
        }
    }
}
