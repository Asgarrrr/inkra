//! The two docks: their toggles, revealing a view, the dock menu, and each view's title and icon (docks spec).

use gpui::{Action, AppContext, Context, Pixels, Point, SharedString, Window, actions};
use inkra_layout::{Side, SplitDirection, Surface, View};

use super::AppShell;
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
}
