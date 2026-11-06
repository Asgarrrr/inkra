//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use std::collections::HashSet;

use gpui::{
    Action, App, AppContext, Context, Focusable, Pixels, Point, SharedString, Window, actions,
};
use inkra_layout::{GroupId, Side, SplitDirection, Surface, View};
use inkra_tabs::TabId;

use super::AppShell;
use crate::sidebar::Sidebar;
use crate::ui::{ContextMenu, IconName};

actions!(
    docks,
    [
        ToggleLeftDock,
        ToggleRightDock,
        MoveFilesToOtherDock,
        MoveBacklinksToOtherDock
    ]
);

/// `view` out of its dock group into a fresh one on its `direction` side (splits spec, S8).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = docks, no_json)]
pub(crate) struct SplitView {
    pub(crate) view: View,
    pub(crate) direction: SplitDirection,
}

/// Each group in `Layout::groups` order: its views in tab order, its active view and its tab count.
type TabState = Vec<(Vec<View>, Option<View>, usize)>;

pub(super) fn title(view: View) -> &'static str {
    match view {
        View::Files => "Files",
        View::Backlinks => "Backlinks",
    }
}

pub(super) fn icon(view: View) -> IconName {
    match view {
        View::Files => IconName::Files,
        View::Backlinks => IconName::Link,
    }
}

impl AppShell {
    pub(super) fn toggle_left_dock(
        &mut self,
        _: &ToggleLeftDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_dock(Side::Left, window, cx);
    }

    pub(super) fn toggle_right_dock(
        &mut self,
        _: &ToggleRightDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_dock(Side::Right, window, cx);
    }

    pub(super) fn move_files_to_other_dock(
        &mut self,
        _: &MoveFilesToOtherDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_other_dock(View::Files, window, cx);
    }

    pub(super) fn move_backlinks_to_other_dock(
        &mut self,
        _: &MoveBacklinksToOtherDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_other_dock(View::Backlinks, window, cx);
    }

    pub(super) fn split_view(
        &mut self,
        &SplitView { view, direction }: &SplitView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (group, tab) = self.tab_of(view);
        if self.group(group).tabs.count() >= 2 {
            self.split_off((group, tab), group, direction, window, cx);
        }
    }

    fn move_to_other_dock(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.side_of(view).other();
        self.append(view, side, window, cx);
    }

    /// `view`'s menu: move it to the other dock, split it off a group it shares, or hide its dock.
    pub(super) fn open_dock_menu(
        &mut self,
        view: View,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes.is_none() {
            return;
        }
        let side = self.side_of(view);
        let move_label = match side.other() {
            Side::Left => "Move to Left Dock",
            Side::Right => "Move to Right Dock",
        };
        let move_action: Box<dyn Action> = match view {
            View::Files => Box::new(MoveFilesToOtherDock),
            View::Backlinks => Box::new(MoveBacklinksToOtherDock),
        };
        let hide_action: Box<dyn Action> = match side {
            Side::Left => Box::new(ToggleLeftDock),
            Side::Right => Box::new(ToggleRightDock),
        };
        let mut entries = vec![(SharedString::from(move_label), move_action)];
        if self.group(self.tab_of(view).0).tabs.count() > 1 {
            for (label, direction) in [
                ("Split Right", SplitDirection::Right),
                ("Split Down", SplitDirection::Down),
            ] {
                let split: Box<dyn Action> = Box::new(SplitView { view, direction });
                entries.push((SharedString::from(label), split));
            }
        }
        entries.push((SharedString::from("Hide Dock"), hide_action));
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
    }

    pub(super) fn toggle_dock(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, _, _| shell.layout.toggle(side));
    }

    /// Activates `view` in its group, and shows its dock or makes its centre group the active one.
    pub(super) fn reveal(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, window, cx| {
            let (group, tab) = shell.tab_of(view);
            shell.activate_tab(group, tab, window, cx);
            match shell.layout.surface_of(group) {
                Surface::Center => shell.switch_group(group, cx),
                Surface::Dock(side) => shell.layout.show(side),
            }
        });
    }

    /// Docks rules 1 and 2, and a view into the centre (P4). `index` counts in group `to` after
    /// `view` left it. A centre group `to` becomes the active one and takes the focus.
    ///
    /// # Panics
    /// If `index` is past the end of that group.
    pub(super) fn move_view(
        &mut self,
        view: View,
        to: GroupId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let surface = self.layout.surface_of(to);
        self.change_layout(window, cx, |shell, window, cx| {
            let (source, tab) = shell.tab_of(view);
            let len = shell.group(to).tabs.count() - usize::from(source == to);
            assert!(
                index <= len,
                "index {index} is past the end of a dock group of {len}"
            );
            if source == to {
                shell.group_mut(source).tabs.move_tab(tab, index);
                shell.activate_tab(source, tab, window, cx);
            } else {
                let detached = shell.detach(source, tab, window, cx);
                shell.attach(detached, to, index, window, cx);
            }
            match surface {
                Surface::Center => shell.switch_group(to, cx),
                Surface::Dock(side) => shell.layout.show(side),
            }
        });
        if surface == Surface::Center
            && let Some(focus) = self.land_focus(cx)
        {
            window.focus(&focus, cx);
        }
    }

    /// Moves `view` to the end of `side`'s first group, as `move_view` does; to a fresh dock on an empty side.
    pub(super) fn append(
        &mut self,
        view: View,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(&group) = self.layout.dock_groups(side).first() {
            self.drop_view_at_end(view, group, window, cx);
            return;
        }
        self.change_layout(window, cx, |shell, window, cx| {
            let (source, tab) = shell.tab_of(view);
            let detached = shell.detach(source, tab, window, cx);
            let group = shell.layout.add_dock(side);
            shell.attach_alone(detached, group, window, cx);
            shell.layout.show(side);
        });
    }

    /// It is its group's active view, and that group is in the centre or a drawn dock.
    pub(super) fn is_visible(&self, view: View) -> bool {
        let (group, _) = self.tab_of(view);
        let drawn = match self.layout.surface_of(group) {
            Surface::Center => true,
            Surface::Dock(side) => self.layout.is_drawn(side),
        };
        drawn && self.group(group).shown.view() == Some(view)
    }

    /// Its dock's side; a view in the centre counts as on its default side.
    pub(super) fn side_of(&self, view: View) -> Side {
        match self.layout.surface_of(self.tab_of(view).0) {
            Surface::Center => view.default_side(),
            Surface::Dock(side) => side,
        }
    }

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
            if focused && let Some(focus) = self.land_focus(cx) {
                window.focus(&focus, cx);
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
