//! The docks' chrome: each column, the view icons that switch and move its views, and the toggles that show and hide it (docks spec).

use gpui::{
    AnyElement, App, AppContext, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use inkra_layout::{GroupId, Side, SplitPath, Surface, View};

use super::dock_drag::DraggedView;
use super::docks::{ToggleLeftDock, ToggleRightDock, icon, title};
use super::drop_marker;
use super::group::selector;
use super::members::Slot;
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::{NewFolder, NewNote};
use crate::theme;
use crate::ui::{IconButton, IconName, PanelHeader, Tooltip, h_flex, keep_focus, lower_header};
use crate::vault_search::SearchVault;

fn icon_id(view: View) -> &'static str {
    match view {
        View::Files => "dock-icon-files",
        View::Backlinks => "dock-icon-backlinks",
    }
}

pub(super) fn name(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

fn column_selector(side: Side) -> &'static str {
    match side {
        Side::Left => "dock-left",
        Side::Right => "dock-right",
    }
}

pub(super) fn width(side: Side) -> Pixels {
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
        self.panes.as_ref()?;
        if !self.layout.is_drawn(side) {
            return None;
        }
        let root = self.layout.dock(side)?;
        let path = SplitPath {
            surface: Surface::Dock(side),
            steps: Vec::new(),
        };
        let top = self.layout.top_groups(path.surface);
        let colors = theme::current(cx).colors;
        let column = side_column(column_selector(side), width(side))
            .bg(colors.background)
            .border_color(colors.border_variant)
            .flex()
            // Over the chrome, not in its place: a bare tint would show the window's blur.
            .drag_over::<DraggedView>(move |style, _, _, _| {
                style.bg(colors.background.blend(colors.accent_background))
            })
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    shell.append(dragged.view, side, window, cx);
                }),
            );
        let column = match side {
            Side::Left => column.border_r(theme::RULE_WIDTH),
            Side::Right => column.border_l(theme::RULE_WIDTH),
        };
        Some(column.child(self.render_member(root, Slot::ROOT, &path, &top, &mut 0, cx)))
    }

    /// Dock group `k`'s column: its header if it is lower, its view's header, and the view, which a
    /// view dropped near its edge splits.
    pub(super) fn render_view_column(
        &self,
        side: Side,
        k: usize,
        group: GroupId,
        (slot, path): (Slot, &SplitPath),
        lower: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let view = self.group(group).shown.view();
        let base =
            div().debug_selector(move || selector(format_args!("dock-{}-group", name(side)), k));
        let header = lower.then(|| {
            lower_header(cx)
                .child(div().flex_none().w(theme::ROW_INSET))
                .children(self.render_dock_strip(side, group, k, cx))
        });
        let rows = view.map(|view| self.render_view_header(view, cx));
        let body = self.group_body(
            group,
            selector(format_args!("dock-{}-drop-overlay", name(side)), k),
            view.and_then(|view| self.render_view_body(view)),
            cx,
        );
        Self::group_column(base, (slot, path), k, header, rows, body, cx)
    }

    /// What `view` draws under its header; `None` with no vault.
    pub(super) fn render_view_body(&self, view: View) -> Option<AnyElement> {
        let panes = self.panes.as_ref()?;
        Some(match view {
            View::Files => panes.sidebar.clone().into_any_element(),
            View::Backlinks => panes.backlinks.clone().into_any_element(),
        })
    }

    /// The view's own first row. A press keeps the focus, as every header's does.
    pub(super) fn render_view_header(&self, view: View, cx: &mut Context<Self>) -> AnyElement {
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

    /// Dock group `k`'s view icons; `None` when the dock is not drawn.
    pub(super) fn render_dock_strip(
        &self,
        side: Side,
        group: GroupId,
        k: usize,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.panes.is_none() || !self.layout.is_drawn(side) {
            return None;
        }
        let active = self.group(group).shown.view();
        Some(
            h_flex()
                .debug_selector(move || selector(format_args!("dock-bar-{}", name(side)), k))
                .flex_none()
                .items_center()
                .children(
                    self.group(group)
                        .views()
                        .enumerate()
                        .map(|(ix, (_, view))| {
                            Self::render_dock_icon(view, Some(view) == active, group, ix, cx)
                        }),
                ),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_layout` drops an equal layout.
    /// A drop on it inserts at `ix`, counted after the dragged view left (`Tabs::move_tab`'s rule).
    fn render_dock_icon(
        view: View,
        active: bool,
        group: GroupId,
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
            .on_drag(
                DraggedView {
                    view,
                    index: ix,
                    group,
                },
                |dragged, _, _, cx| cx.new(|_| dragged.clone()),
            )
            .drag_over::<DraggedView>(move |style, dragged, _, _| {
                drop_marker::mark(style, dragged.landing(group, ix), &colors)
            })
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    shell.move_view(dragged.view, group, ix, window, cx);
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
