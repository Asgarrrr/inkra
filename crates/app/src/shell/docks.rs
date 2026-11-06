//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use std::collections::HashSet;

use gpui::{
    Action, App, AppContext, Context, Focusable, Pixels, Point, SharedString, Window, actions,
};
use inkra_layout::{Axis, GroupId, Side, Surface, View};
use inkra_tabs::{Closed, TabId};

use super::AppShell;
use super::group::Group;
use super::parked::Parked;
use super::tabs::{Place, capture};
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

/// `view` out of its dock group into a fresh one after it along `axis` (splits spec, S8).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = docks, no_json)]
pub(crate) struct SplitView {
    pub(crate) view: View,
    pub(crate) axis: Axis,
}

/// Each dock group, depth-first, left then right: its views in tab order and its active view.
type DockState = Vec<(GroupId, Vec<View>, Option<View>)>;

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
        &SplitView { view, axis }: &SplitView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_layout(window, cx, |shell, window, cx| {
            let (group, tab) = shell.tab_of(view);
            if shell.group(group).tabs.count() < 2 || !shell.fits_split(group, axis, window) {
                return;
            }
            shell.take_view_tab(group, tab, window, cx);
            let fresh = shell.layout.split(group, axis);
            shell.groups.insert(fresh, Group::holding(&[view], 0));
        });
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
            for (label, axis) in [
                ("Split Right", Axis::Horizontal),
                ("Split Down", Axis::Vertical),
            ] {
                let split: Box<dyn Action> = Box::new(SplitView { view, axis });
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

    /// Activates `view` in its dock and shows that dock.
    pub(super) fn reveal(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, window, cx| {
            let side = shell.side_of(view);
            let (group, tab) = shell.tab_of(view);
            shell.activate_tab(group, tab, window, cx);
            shell.layout.show(side);
        });
    }

    /// Docks rules 1 and 2. `index` counts in dock group `to` after `view` left it.
    ///
    /// # Panics
    /// If `index` is past the end of that group, or `to` is not a dock group.
    pub(super) fn move_view(
        &mut self,
        view: View,
        to: GroupId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Surface::Dock(side) = self.layout.surface_of(to) else {
            panic!("{to:?} is not a dock group");
        };
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
                shell.take_view_tab(source, tab, window, cx);
                shell.put_view_tab(view, to, index, window, cx);
            }
            shell.layout.show(side);
        });
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
            let index = self.group(group).tabs.count() - usize::from(self.tab_of(view).0 == group);
            self.move_view(view, group, index, window, cx);
            return;
        }
        self.change_layout(window, cx, |shell, window, cx| {
            let (source, tab) = shell.tab_of(view);
            shell.take_view_tab(source, tab, window, cx);
            let group = shell.layout.add_dock(side);
            shell.groups.insert(group, Group::holding(&[view], 0));
            shell.layout.show(side);
        });
    }

    /// A dock's last tab takes its group with it (docks rule 3).
    fn take_view_tab(
        &mut self,
        group: GroupId,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group(group).tabs.count() == 1 {
            self.layout.remove(group);
            self.groups.remove(&group);
            return;
        }
        let Group { tabs, shown, .. } = self.group_mut(group);
        let closed = tabs.close(tab, |slot| match slot {
            None => capture(shown, cx),
            Some(parked) => parked.unpark(cx),
        });
        if let Some(Closed {
            shown: Some((_, parked)),
            ..
        }) = closed
        {
            let place = parked.unpark(cx);
            self.display(group, place, window, cx);
        }
    }

    fn put_view_tab(
        &mut self,
        view: View,
        group: GroupId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Group { tabs, shown, .. } = self.group_mut(group);
        tabs.insert(index, || Parked::park(shown, cx));
        self.display(group, Place::View(view), window, cx);
    }

    /// Its dock is drawn and it is its group's active view.
    pub(super) fn is_visible(&self, view: View) -> bool {
        let (group, _) = self.tab_of(view);
        self.layout.is_drawn(self.side_of(view)) && self.group(group).shown.view() == Some(view)
    }

    pub(super) fn side_of(&self, view: View) -> Side {
        let Surface::Dock(side) = self.layout.surface_of(self.tab_of(view).0) else {
            unreachable!("a view sits in a dock group");
        };
        side
    }

    /// # Panics
    /// Only on a broken invariant: each view sits in exactly one dock tab.
    fn tab_of(&self, view: View) -> (GroupId, TabId) {
        [Side::Left, Side::Right]
            .into_iter()
            .flat_map(|side| self.layout.dock_groups(side))
            .find_map(|group| {
                let (tab, _) = self.group(group).views().find(|&(_, held)| held == view)?;
                Some((group, tab))
            })
            .unwrap_or_else(|| panic!("{view:?} sits in no dock"))
    }

    fn dock_state(&self) -> DockState {
        [Side::Left, Side::Right]
            .into_iter()
            .flat_map(|side| self.layout.dock_groups(side))
            .map(|group| {
                let group_views = self.group(group).views().map(|(_, view)| view).collect();
                (group, group_views, self.group(group).shown.view())
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
        let (layout, docks) = (self.layout.clone(), self.dock_state());
        let files_shown = self.is_visible(View::Files);
        change(self, window, cx);
        if self.layout == layout && self.dock_state() == docks {
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
            assert_eq!(
                self.group(group).views().count(),
                0,
                "the centre holds no view"
            );
            assert!(
                self.editor_of(group)
                    .is_none_or(|editor| editor.read(cx).is_pinned() == (group != last)),
                "an editor is pinned exactly when its group is not the last active"
            );
        }
        let mut views = Vec::new();
        for side in [Side::Left, Side::Right] {
            for group in self.layout.dock_groups(side) {
                let group = self.group(group);
                assert_eq!(
                    group.views().count(),
                    group.tabs.count(),
                    "a dock tab holds a view"
                );
                views.extend(group.views().map(|(_, view)| view));
            }
        }
        for view in View::ALL {
            let tabs = views.iter().filter(|&&held| held == view).count();
            assert_eq!(tabs, 1, "{view:?} sits in exactly one tab");
        }
    }
}
