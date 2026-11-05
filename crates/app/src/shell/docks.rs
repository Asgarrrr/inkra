//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use gpui::{
    AnyElement, Context, Div, Focusable, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, StatefulInteractiveElement, Styled, Window, actions, div,
    prelude::FluentBuilder,
};
use inkra_layout::{Docks, Side, View};

use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::Sidebar;
use crate::theme;
use crate::ui::{IconButton, IconName, PanelHeader, Tooltip, keep_focus};

actions!(
    docks,
    [
        ToggleLeftDock,
        ToggleRightDock,
        MoveFilesToOtherDock,
        MoveBacklinksToOtherDock
    ]
);

fn title(view: View) -> &'static str {
    match view {
        View::Files => "Files",
        View::Backlinks => "Backlinks",
    }
}

fn icon(view: View) -> IconName {
    match view {
        View::Files => IconName::Files,
        View::Backlinks => IconName::Link,
    }
}

fn icon_id(view: View) -> &'static str {
    match view {
        View::Files => "dock-icon-files",
        View::Backlinks => "dock-icon-backlinks",
    }
}

fn bar_selector(side: Side) -> &'static str {
    match side {
        Side::Left => "dock-bar-left",
        Side::Right => "dock-bar-right",
    }
}

fn header_selector(view: View) -> &'static str {
    match view {
        View::Files => "panel-header-files",
        View::Backlinks => "panel-header-backlinks",
    }
}

fn column_selector(side: Side) -> &'static str {
    match side {
        Side::Left => "dock-left",
        Side::Right => "dock-right",
    }
}

fn width(side: Side) -> Pixels {
    match side {
        Side::Left => LEFT_DOCK_WIDTH,
        Side::Right => RIGHT_DOCK_WIDTH,
    }
}

/// A dock's column: fixed width, the body's full height.
fn side_column(selector: &'static str, width: Pixels) -> Div {
    div()
        .debug_selector(move || selector.into())
        .flex_none()
        .w(width)
        .h_full()
}

impl AppShell {
    pub(super) fn toggle_left_dock(
        &mut self,
        _: &ToggleLeftDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_docks(window, cx, |docks| docks.toggle(Side::Left));
    }

    pub(super) fn toggle_right_dock(
        &mut self,
        _: &ToggleRightDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_docks(window, cx, |docks| docks.toggle(Side::Right));
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
        self.change_docks(window, cx, |docks| {
            let side = docks.side_of(view).other();
            let at = docks.dock(side).views().len();
            docks.move_view(view, side, at);
        });
    }

    /// The one writer of `docks`. A hidden tree keeps neither focus nor an open name field: either
    /// would take keys out of sight.
    pub(super) fn change_docks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Docks),
    ) {
        let Some(panes) = &mut self.panes else {
            return;
        };
        let before = panes.docks.clone();
        change(&mut panes.docks);
        if panes.docks == before {
            return;
        }
        if before.is_visible(View::Files) && !panes.docks.is_visible(View::Files) {
            let sidebar = panes.sidebar.clone();
            let focused = sidebar.focus_handle(cx).contains_focused(window, cx);
            sidebar.update(cx, Sidebar::cancel_edit);
            if focused && let Some(focus) = self.land_focus(cx) {
                window.focus(&focus, cx);
            }
        }
        cx.notify();
    }

    /// `None` with no vault or an undrawn dock. A hidden view keeps its entity, so it comes back as it was left.
    pub(super) fn render_dock(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let panes = self.panes.as_ref()?;
        let dock = panes.docks.dock(side);
        if !dock.is_drawn() {
            return None;
        }
        let view = dock.active()?;
        let (body, count) = match view {
            View::Files => (panes.sidebar.clone().into_any_element(), None),
            View::Backlinks => (
                panes.backlinks.clone().into_any_element(),
                panes.backlinks.read(cx).count(),
            ),
        };
        let colors = theme::current(cx).colors;
        let column = side_column(column_selector(side), width(side))
            .bg(colors.background)
            .border_color(colors.border_variant)
            .flex()
            .flex_col();
        let column = match side {
            Side::Left => column.border_r(theme::RULE_WIDTH),
            Side::Right => column.border_l(theme::RULE_WIDTH),
        };
        let header = match dock.views() {
            [_] => PanelHeader::new(header_selector(view), title(view))
                .when_some(count, PanelHeader::count),
            views => PanelHeader::bar(bar_selector(side)).children(
                views
                    .iter()
                    .map(|&held| Self::render_dock_icon(held, held == view, cx)),
            ),
        };
        Some(
            column
                .child(header.on_mouse_down(MouseButton::Left, keep_focus))
                .child(div().flex_1().min_h_0().child(body)),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_docks` drops an equal layout.
    fn render_dock_icon(view: View, active: bool, cx: &mut Context<Self>) -> AnyElement {
        IconButton::new(icon_id(view), icon(view), title(view))
            .toggled(active)
            .tooltip(Tooltip::text(title(view)))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.change_docks(window, cx, |docks| docks.reveal(view));
            }))
            .into_any_element()
    }

    /// `None` with no vault or an empty dock: there is no dock to toggle (rule 3).
    pub(super) fn render_dock_toggles(
        &self,
        cx: &mut Context<Self>,
    ) -> (
        Option<impl IntoElement + use<>>,
        Option<impl IntoElement + use<>>,
    ) {
        let docks = self.panes.as_ref().map(|panes| &panes.docks);
        let has_views = |side| docks.is_some_and(|docks| !docks.dock(side).views().is_empty());
        let shown = |side| docks.is_some_and(|docks| docks.dock(side).is_shown());
        let left = has_views(Side::Left).then(|| {
            IconButton::new("dock-toggle-left", IconName::PanelLeft, "Toggle left dock")
                .toggled(shown(Side::Left))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.toggle_left_dock(&ToggleLeftDock, window, cx);
                }))
        });
        let right = has_views(Side::Right).then(|| {
            IconButton::new(
                "dock-toggle-right",
                IconName::PanelRight,
                "Toggle right dock",
            )
            .toggled(shown(Side::Right))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_right_dock(&ToggleRightDock, window, cx);
            }))
        });
        (left, right)
    }
}
