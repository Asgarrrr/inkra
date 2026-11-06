//! A dock view on the move: what a drag carries and the preview under the pointer (docks spec, Drag).

use gpui::{Context, InteractiveElement, IntoElement, Pixels, Render, Styled, Window, div, px};
use inkra_layout::{Side, View};

use super::AppShell;
use super::docks::{icon, title};
use crate::theme;
use crate::ui::IconButton;

/// The root's group, so a strip can tint on a drag over it without a tab drag lighting it.
pub(super) const SHELL_GROUP: &str = "app-shell";

const DROP_STRIP_WIDTH: Pixels = px(40.);

/// The view under the pointer; also its preview. `index` is where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedView {
    pub(super) view: View,
    pub(super) index: usize,
}

impl Render for DraggedView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("dock-view-preview", icon(self.view), title(self.view)).toggled(true)
    }
}

impl AppShell {
    /// A drag-only target at the window edge of a side with no views: without it an emptied dock could not be refilled.
    pub(super) fn render_drop_strip(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if self.panes.is_none() || !cx.has_active_drag() || self.layout.dock(side).is_some() {
            return None;
        }
        let colors = theme::current(cx).colors;
        let strip = div().absolute().top_0().bottom_0();
        let (strip, name) = match side {
            Side::Left => (strip.left_0().border_r(theme::DROP_EDGE), "left"),
            Side::Right => (strip.right_0().border_l(theme::DROP_EDGE), "right"),
        };
        Some(
            strip
                .w(DROP_STRIP_WIDTH)
                .debug_selector(move || format!("dock-drop-{name}"))
                .group_drag_over::<DraggedView>(SHELL_GROUP, move |style| {
                    style.bg(colors.accent_background)
                })
                .drag_over::<DraggedView>(move |style, _, _, _| {
                    style
                        .bg(colors.accent_background)
                        .border_color(colors.accent)
                })
                .on_drop(
                    cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                        shell.append(dragged.view, side, window, cx);
                    }),
                ),
        )
    }
}
