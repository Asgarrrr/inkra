//! The two docks: their toggles, revealing a view, the dock menu, and the Files pane (docks spec).

use gpui::{Action, AppContext, Context, Pixels, Point, Window, actions};
use inkra_layout::{Side, SplitDirection, Surface, ViewId};

use super::AppShell;
use crate::pane::{CORE_PANES, PaneDecl, boxed};
use crate::ui::{ContextMenu, IconName, MenuEntry};

actions!(
    docks,
    [ToggleLeftDock, ToggleRightDock, MoveFilesToOtherDock]
);

pub(crate) const FILES: ViewId = ViewId::new("files");

pub(crate) static FILES_PANE: PaneDecl = PaneDecl {
    id: FILES,
    title: "Files",
    icon: IconName::Files,
    icon_id: "dock-icon-files",
    header_id: "panel-header-files",
    side: Side::Left,
    move_action: boxed::<MoveFilesToOtherDock>,
};

/// `view` out of its dock group into a fresh one on its `direction` side (splits spec, S8).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = docks, no_json)]
pub(crate) struct SplitView {
    pub(crate) view: ViewId,
    pub(crate) direction: SplitDirection,
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

    #[expect(
        clippy::unused_self,
        reason = "the feature registry slice reads the shell's own panes here"
    )]
    pub(super) fn pane(&self, view: ViewId) -> Option<&'static PaneDecl> {
        CORE_PANES.iter().copied().find(|pane| pane.id == view)
    }

    pub(super) fn move_to_other_dock(
        &mut self,
        view: ViewId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(side) = self.side_of(view) else {
            return;
        };
        self.append(view, side.other(), window, cx);
    }

    /// `view`'s menu: move it to the other dock, split it off a group it shares, or hide its dock.
    pub(super) fn open_dock_menu(
        &mut self,
        view: ViewId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes.is_none() {
            return;
        }
        let (Some(side), Some(pane), Some((group, _))) =
            (self.side_of(view), self.pane(view), self.tab_of(view))
        else {
            return;
        };
        let move_label = match side.other() {
            Side::Left => "Move to Left Dock",
            Side::Right => "Move to Right Dock",
        };
        let hide_action: Box<dyn Action> = match side {
            Side::Left => Box::new(ToggleLeftDock),
            Side::Right => Box::new(ToggleRightDock),
        };
        let mut entries = vec![MenuEntry::new(move_label, (pane.move_action)())];
        if self.group(group).tabs.count() > 1 {
            for (label, direction) in [
                ("Split Right", SplitDirection::Right),
                ("Split Down", SplitDirection::Down),
            ] {
                let split: Box<dyn Action> = Box::new(SplitView { view, direction });
                entries.push(MenuEntry::new(label, split));
            }
        }
        entries.push(MenuEntry::new("Hide Dock", hide_action));
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
    }

    pub(super) fn toggle_dock(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, _, _| shell.layout.toggle(side));
    }

    /// Activates `view` in its group, and shows its dock or makes its centre group the active one.
    pub(super) fn reveal(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, window, cx| {
            let Some((group, tab)) = shell.tab_of(view) else {
                return;
            };
            shell.activate_tab(group, tab, window, cx);
            match shell.layout.surface_of(group) {
                Surface::Center => shell.switch_group(group, cx),
                Surface::Dock(side) => shell.layout.show(side),
            }
        });
    }

    /// It is its group's active view, and that group is in the centre or a drawn dock.
    pub(super) fn is_visible(&self, view: ViewId) -> bool {
        let Some((group, _)) = self.tab_of(view) else {
            return false;
        };
        let drawn = match self.layout.surface_of(group) {
            Surface::Center => true,
            Surface::Dock(side) => self.is_dock_drawn(side),
        };
        drawn && self.group(group).shown.view() == Some(view)
    }

    /// Its dock's side; a view in the centre counts as on its pane's side.
    pub(super) fn side_of(&self, view: ViewId) -> Option<Side> {
        let (group, _) = self.tab_of(view)?;
        match self.layout.surface_of(group) {
            Surface::Center => self.pane(view).map(|pane| pane.side),
            Surface::Dock(side) => Some(side),
        }
    }
}
