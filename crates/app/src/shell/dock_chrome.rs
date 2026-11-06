//! The docks' chrome: each column, the view icons that switch and move its views, and the toggles that show and hide it (docks spec).

use std::cmp::Ordering;

use gpui::{
    AnyElement, App, AppContext, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use inkra_layout::{Side, View};

use super::dock_drag::DraggedView;
use super::docks::{ToggleLeftDock, ToggleRightDock, icon, title};
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::{NewFolder, NewNote};
use crate::theme;
use crate::ui::{IconButton, IconName, PanelHeader, Tooltip, h_flex, keep_focus};
use crate::vault_search::SearchVault;

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
    /// `None` with no vault or an undrawn dock. A hidden view keeps its entity, so it comes back as it was left.
    pub(super) fn render_dock(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let panes = self.panes.as_ref()?;
        if !self.layout.is_drawn(side) {
            return None;
        }
        let view = self.group(self.layout.dock(side)?).shown.view()?;
        let body = match view {
            View::Files => panes.sidebar.clone().into_any_element(),
            View::Backlinks => panes.backlinks.clone().into_any_element(),
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
                    shell.append(dragged.view, side, window, cx);
                }),
            );
        let column = match side {
            Side::Left => column.border_r(theme::RULE_WIDTH),
            Side::Right => column.border_l(theme::RULE_WIDTH),
        };
        Some(
            column
                .child(self.render_view_header(view, cx))
                .child(div().flex_1().min_h_0().child(body)),
        )
    }

    /// The view's own first row. A press keeps the focus, as every header's does.
    fn render_view_header(&self, view: View, cx: &mut Context<Self>) -> AnyElement {
        match view {
            View::Files => {
                PanelHeader::bar("panel-header-files")
                    .child(
                        IconButton::new("sidebar-new-note", IconName::PenLine, "New note")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_note(&NewNote, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("sidebar-new-folder", IconName::FolderPlus, "New folder")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_folder(&NewFolder, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("sidebar-search", IconName::Search, "Search vault")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.search_vault(&SearchVault, window, cx);
                            })),
                    )
                    .on_mouse_down(MouseButton::Left, keep_focus)
                    .into_any_element()
            }
            View::Backlinks => PanelHeader::new("panel-header-backlinks", title(view))
                .when_some(
                    self.panes
                        .as_ref()
                        .and_then(|panes| panes.backlinks.read(cx).count()),
                    PanelHeader::count,
                )
                .on_mouse_down(MouseButton::Left, keep_focus)
                .into_any_element(),
        }
    }

    /// `None` when the dock is not drawn.
    pub(super) fn render_dock_strip(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.panes.is_none() || !self.layout.is_drawn(side) {
            return None;
        }
        let group = self.group(self.layout.dock(side)?);
        let active = group.shown.view();
        Some(
            h_flex()
                .debug_selector(move || bar_selector(side).into())
                .flex_none()
                .items_center()
                .children(group.views().enumerate().map(|(ix, (_, held))| {
                    Self::render_dock_icon(held, Some(held) == active, side, ix, cx)
                })),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_layout` drops an equal layout.
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
                shell.reveal(view, window, cx);
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
                    shell.move_view(dragged.view, side, ix, window, cx);
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
        let has_views = |side| self.panes.is_some() && self.layout.dock(side).is_some();
        let left = has_views(Side::Left).then(|| {
            IconButton::new("dock-toggle-left", IconName::PanelLeft, "Toggle left dock")
                .toggled(self.layout.is_shown(Side::Left))
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
            .toggled(self.layout.is_shown(Side::Right))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_right_dock(&ToggleRightDock, window, cx);
            }))
        });
        (left, right)
    }
}
