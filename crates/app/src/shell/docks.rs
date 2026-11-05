//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use gpui::{
    Context, Div, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Styled, Window, actions, div, prelude::FluentBuilder,
};
use inkra_layout::{Docks, Side, View};

use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::Sidebar;
use crate::theme;
use crate::ui::{IconButton, IconName, PanelHeader, keep_focus};

actions!(docks, [ToggleLeftDock, ToggleRightDock]);

fn title(view: View) -> &'static str {
    match view {
        View::Files => "Files",
        View::Backlinks => "Backlinks",
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

    /// `None` with no vault or a hidden dock. A hidden view keeps its entity, so it comes back as it was left.
    pub(super) fn render_dock(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let panes = self.panes.as_ref()?;
        let dock = panes.docks.dock(side);
        if !dock.is_shown() {
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
        Some(
            column
                .child(
                    PanelHeader::new(header_selector(view), title(view))
                        .when_some(count, PanelHeader::count)
                        .on_mouse_down(MouseButton::Left, keep_focus),
                )
                .child(div().flex_1().min_h_0().child(body)),
        )
    }

    /// `None` with no vault: there is no dock to toggle.
    pub(super) fn render_dock_toggles(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(impl IntoElement + use<>, impl IntoElement + use<>)> {
        let docks = &self.panes.as_ref()?.docks;
        let left = IconButton::new("dock-toggle-left", IconName::PanelLeft, "Toggle left dock")
            .toggled(docks.dock(Side::Left).is_shown())
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_left_dock(&ToggleLeftDock, window, cx);
            }));
        let right = IconButton::new(
            "dock-toggle-right",
            IconName::PanelRight,
            "Toggle right dock",
        )
        .toggled(docks.dock(Side::Right).is_shown())
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.toggle_right_dock(&ToggleRightDock, window, cx);
        }));
        Some((left, right))
    }
}
