//! A dock view on the move: what a drag carries and the preview under the pointer (docks spec, Drag).

use std::cmp::Ordering;

use gpui::{
    Context, InteractiveElement, IntoElement, Pixels, Point, Render, Styled, Window, div, px,
};
use inkra_layout::{GroupId, Side, ViewId};

use super::AppShell;
use super::drop_marker;
use crate::pane::PaneDecl;
use crate::theme;
use crate::ui::IconButton;

/// The root's group, so a strip can show its edge on a drag over it without a tab drag lighting it.
pub(super) const SHELL_GROUP: &str = "app-shell";

const DROP_STRIP_WIDTH: Pixels = px(40.);

/// The view under the pointer; also its preview. `index` and `group` are where it stood when the
/// drag began.
#[derive(Clone)]
pub(super) struct DraggedView {
    pub(super) view: ViewId,
    pub(super) decl: &'static PaneDecl,
    pub(super) index: usize,
    pub(super) group: GroupId,
}

impl DraggedView {
    /// Where slot `ix` of `group` sits from this view: `Less` marks its left edge, `Greater` its
    /// right. A view from another group lands before the slot, as a foreign tab does.
    pub(super) fn landing(&self, group: GroupId, ix: usize) -> Ordering {
        drop_marker::landing((self.group == group).then_some(self.index), ix)
    }
}

impl Render for DraggedView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("dock-view-preview", self.decl.icon, self.decl.title).toggled(true)
    }
}

impl AppShell {
    /// Whether a drag draws `side`'s drop strip: a vault is open and the side has no views.
    fn has_drop_strip(&self, side: Side) -> bool {
        self.panes.is_some() && self.layout.dock(side).is_none()
    }

    /// Whether a point falls in a drawn drop strip, which takes a view's drop from any group under it.
    pub(super) fn drop_strip_hit(&self) -> impl Fn(Point<Pixels>, &Window) -> bool + 'static {
        let (left, right) = (
            self.has_drop_strip(Side::Left),
            self.has_drop_strip(Side::Right),
        );
        move |at, window| {
            (left && at.x < DROP_STRIP_WIDTH)
                || (right && at.x >= window.viewport_size().width - DROP_STRIP_WIDTH)
        }
    }

    /// A drag-only target at the window edge of a side with no views: without it an emptied dock
    /// could not be refilled. A view's drag shows its edge; the pointer over it lights it.
    pub(super) fn render_drop_strip(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !cx.has_active_drag() || !self.has_drop_strip(side) {
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
                    style.border_color(colors.border)
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
