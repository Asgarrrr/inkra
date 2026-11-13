//! The docks' chrome: each column, the view icons that switch and move its views, and the toggles that show and hide it (docks spec).

use gpui::{
    AnyElement, App, AppContext, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use inkra_layout::{GroupId, Side, SplitPath, Surface, ViewId};

use super::dock_drag::DraggedView;
use super::docks::{ToggleLeftDock, ToggleRightDock};
use super::drop_marker;
use super::group::selector;
use super::members::Slot;
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::pane::{PaneDecl, PaneHeader};
use crate::theme;
use crate::ui::{IconButton, IconName, PanelHeader, Tooltip, h_flex, keep_focus, lower_header};

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
    /// With no vault nothing draws a dock, whatever the layout holds.
    pub(super) fn is_dock_drawn(&self, side: Side) -> bool {
        self.panes.is_some() && self.layout.is_drawn(side)
    }

    /// No vault or an empty dock: there is no dock to toggle (rule 3).
    pub(super) fn has_dock_toggle(&self, side: Side) -> bool {
        self.panes.is_some() && self.layout.dock(side).is_some()
    }

    /// `None` with no vault or an undrawn dock. A hidden view keeps its entity, so it comes back as it was left.
    pub(super) fn render_dock(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.is_dock_drawn(side) {
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
                    shell.append(dragged.decl.id, side, window, cx);
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
        let rows = view.and_then(|view| self.render_view_header(view, cx));
        let body = self.group_body(
            group,
            selector(format_args!("dock-{}-drop-overlay", name(side)), k),
            view.and_then(|view| self.render_view_body(view)),
            cx,
        );
        Self::group_column(base, (slot, path), k, header, rows, body, cx)
    }

    /// What `view` draws under its header: an empty body until its feature registers it; `None` with no vault.
    pub(super) fn render_view_body(&self, view: ViewId) -> Option<AnyElement> {
        let panes = self.panes.as_ref()?;
        Some(match panes.entries.get(&view) {
            Some(entry) => entry.view().into_any_element(),
            None => div().into_any_element(),
        })
    }

    /// The view's own first row. A press keeps the focus, as every header's does.
    pub(super) fn render_view_header(&self, view: ViewId, cx: &App) -> Option<AnyElement> {
        let decl = self.pane(view)?;
        let header = self
            .pane_entry(view)
            .map_or(PaneHeader::Title { count: None }, |entry| entry.header(cx));
        let (header, own) = match header {
            PaneHeader::Title { count } => (
                PanelHeader::new(decl.header_id, decl.title).when_some(count, PanelHeader::count),
                &[][..],
            ),
            PaneHeader::Buttons(buttons) => (PanelHeader::bar(decl.header_id), buttons),
        };
        let contributed = self
            .features
            .enabled()
            .flat_map(|manifest| manifest.buttons)
            .filter(|contributed| contributed.pane == view)
            .map(|contributed| &contributed.button);
        let header = own
            .iter()
            .chain(contributed)
            .fold(header, |header, button| {
                let action = button.action;
                header.child(
                    IconButton::new(button.id, button.icon, button.tooltip)
                        .on_click(move |_, window, cx| window.dispatch_action(action(), cx)),
                )
            });
        Some(
            header
                .on_mouse_down(MouseButton::Left, keep_focus)
                .into_any_element(),
        )
    }

    /// Dock group `k`'s view icons; `None` when the dock is not drawn.
    pub(super) fn render_dock_strip(
        &self,
        side: Side,
        group: GroupId,
        k: usize,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.is_dock_drawn(side) {
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
                        .filter_map(|(ix, (_, view))| {
                            let decl = self.pane(view);
                            debug_assert!(decl.is_some(), "{view:?} has no pane");
                            let active = Some(view) == active;
                            Some(Self::render_dock_icon(decl?, active, group, ix, cx))
                        }),
                ),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_layout` drops an equal layout.
    /// A drop on it inserts at `ix`, counted after the dragged view left (`Tabs::move_tab`'s rule).
    fn render_dock_icon(
        decl: &'static PaneDecl,
        active: bool,
        group: GroupId,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (view, colors) = (decl.id, theme::current(cx).colors);
        IconButton::new(decl.icon_id, decl.icon, decl.title)
            .toggled(active)
            .tooltip(Tooltip::text(decl.title))
            .on_mouse_down(MouseButton::Right, Self::open_menu_on_press(view, cx))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.reveal(view, window, cx);
            }))
            .on_drag(
                DraggedView {
                    decl,
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
                    shell.move_view(dragged.decl.id, group, ix, window, cx);
                }),
            )
            .into_any_element()
    }

    /// `prevent_default`: the root would take focus from the menu on the press (`div.rs:2765-2780`).
    fn open_menu_on_press(
        view: ViewId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
            window.prevent_default();
            shell.open_dock_menu(view, event.position, window, cx);
        })
    }

    /// Drawn only for a `Toggle` end piece, which exists only where there is a dock to toggle.
    pub(super) fn render_dock_toggle(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (id, icon, label) = match side {
            Side::Left => ("dock-toggle-left", IconName::PanelLeft, "Toggle left dock"),
            Side::Right => (
                "dock-toggle-right",
                IconName::PanelRight,
                "Toggle right dock",
            ),
        };
        IconButton::new(id, icon, label)
            .toggled(self.layout.is_shown(side))
            .on_click(cx.listener(move |shell, _, window, cx| match side {
                Side::Left => shell.toggle_left_dock(&ToggleLeftDock, window, cx),
                Side::Right => shell.toggle_right_dock(&ToggleRightDock, window, cx),
            }))
    }
}
