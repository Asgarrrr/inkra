//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use std::cmp::Ordering;

use gpui::{
    Action, AnyElement, App, AppContext, Context, Div, Focusable, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, actions, div, prelude::FluentBuilder,
};
use inkra_layout::{Docks, Side, View};

use super::dock_drag::DraggedView;
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::Sidebar;
use crate::theme;
use crate::ui::{ContextMenu, IconButton, IconName, PanelHeader, Tooltip, keep_focus};

actions!(
    docks,
    [
        ToggleLeftDock,
        ToggleRightDock,
        MoveFilesToOtherDock,
        MoveBacklinksToOtherDock
    ]
);

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
            docks.append(view, side);
        });
    }

    /// `view`'s menu: move it to the other dock, or hide its own.
    fn open_dock_menu(
        &mut self,
        view: View,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panes) = &self.panes else {
            return;
        };
        let side = panes.docks.side_of(view);
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
        let entries = vec![
            (SharedString::from(move_label), move_action),
            (SharedString::from("Hide Dock"), hide_action),
        ];
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
    }

    /// The one writer of `docks`. A hidden tree keeps neither focus nor an open name field: either
    /// would take keys out of sight. An open menu closes: it describes the layout it opened on.
    pub(super) fn change_docks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Docks),
    ) {
        if self.modal_menu().is_some() {
            self.dismiss_modal(window, cx);
        }
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
        self.save_session_later(cx);
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
            .flex_col()
            .drag_over::<DraggedView>(move |style, _, _, _| style.bg(colors.accent_background))
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    let view = dragged.view;
                    shell.change_docks(window, cx, |docks| docks.append(view, side));
                }),
            );
        let column = match side {
            Side::Left => column.border_r(theme::RULE_WIDTH),
            Side::Right => column.border_l(theme::RULE_WIDTH),
        };
        let header =
            match dock.views() {
                [_] => div()
                    .id(("dock-title-drag", usize::from(side == Side::Right)))
                    .on_drag(DraggedView { view, index: 0 }, |dragged, _, _, cx| {
                        cx.new(|_| dragged.clone())
                    })
                    .child(
                        PanelHeader::new(header_selector(view), title(view))
                            .when_some(count, PanelHeader::count)
                            .on_mouse_down(MouseButton::Right, Self::open_menu_on_press(view, cx))
                            .on_mouse_down(MouseButton::Left, keep_focus),
                    )
                    .into_any_element(),
                views => PanelHeader::bar(bar_selector(side))
                    .children(views.iter().enumerate().map(|(ix, &held)| {
                        Self::render_dock_icon(held, held == view, side, ix, cx)
                    }))
                    .on_mouse_down(MouseButton::Left, keep_focus)
                    .into_any_element(),
            };
        Some(
            column
                .child(header)
                .child(div().flex_1().min_h_0().child(body)),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_docks` drops an equal layout.
    /// A drop on it inserts at `ix`, counted after the dragged view left (`Tabs::move_tab`'s rule).
    fn render_dock_icon(
        view: View,
        active: bool,
        side: Side,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::current(cx).colors;
        IconButton::new(icon_id(view), icon(view), title(view))
            .toggled(active)
            .tooltip(Tooltip::text(title(view)))
            .on_mouse_down(MouseButton::Right, Self::open_menu_on_press(view, cx))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.change_docks(window, cx, |docks| docks.reveal(view));
            }))
            .on_drag(DraggedView { view, index: ix }, |dragged, _, _, cx| {
                cx.new(|_| dragged.clone())
            })
            .drag_over::<DraggedView>(move |style, dragged, _, _| {
                let style = style
                    .bg(colors.accent_background)
                    .border_color(colors.accent);
                match ix.cmp(&dragged.index) {
                    Ordering::Less => style.border_l(theme::DROP_EDGE),
                    Ordering::Greater => style.border_r(theme::DROP_EDGE),
                    Ordering::Equal => style,
                }
            })
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    let dragged = dragged.view;
                    shell.change_docks(window, cx, |docks| docks.move_view(dragged, side, ix));
                }),
            )
            .into_any_element()
    }

    /// `prevent_default`: the root would take focus from the menu on the press (`div.rs:2765-2780`).
    fn open_menu_on_press(
        view: View,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
            window.prevent_default();
            shell.open_dock_menu(view, event.position, window, cx);
        })
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
