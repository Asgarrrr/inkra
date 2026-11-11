//! The tab row is the window's title bar: the traffic lights' space, the toggles, the tabs and the drag regions (shell chrome spec, S3).

use gpui::{
    AnyElement, ClickEvent, Context, Div, ElementId, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Stateful, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use inkra_layout::{GroupId, Side, Surface};

use super::AppShell;
use super::dock_chrome::{name, width};
use super::geometry::{EndPiece, Ends, lights_width};
use super::group::selector;
use crate::theme;
use crate::ui::{h_flex, identified, keep_focus, strip_rule, strip_slot};

impl AppShell {
    /// Toggles and tabs keep their own presses: only a leaf region moves the window (S3-D3).
    pub(super) fn render_tab_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let lights = lights_width(window);
        let left_title = self
            .is_dock_drawn(Side::Left)
            .then(|| self.render_title_dock(Side::Left, lights, cx));
        let right_title = self
            .is_dock_drawn(Side::Right)
            .then(|| self.render_title_dock(Side::Right, lights, cx));
        let top = self.top_rects(Surface::Center);
        let count = top.len();
        let segments: Vec<_> = top
            .into_iter()
            .enumerate()
            .map(|(ix, (k, group, width))| {
                let Ends { lead, trail } = self.title_ends(Surface::Center, ix, count);
                let lead = self.render_ends(lead, lights, cx);
                h_flex()
                    .id(("group-strip", group.raw()))
                    .debug_selector(move || selector("group-strip", k))
                    // Basis 0, no padding: the strip's width is its column's share of the centre.
                    .flex_basis(px(0.))
                    .flex_grow(width)
                    .min_w_0()
                    .h_full()
                    .capture_any_mouse_down(cx.listener(move |shell, _, window, cx| {
                        shell.activate_group(group, window, cx);
                    }))
                    .on_drop(Self::view_drop(group, cx))
                    .children(lead)
                    .child(self.render_nav(k, group, strip_slot(), cx))
                    .child(self.render_tab_bar(k, group, cx))
                    .child(Self::render_new_tab(k, cx))
                    .child(
                        Self::render_drag_region(
                            ElementId::Name(selector("title-drag-end", k).into()),
                            cx,
                        )
                        .flex_1()
                        .min_w(theme::TITLE_DRAG_MIN_WIDTH),
                    )
                    .children(self.render_ends(trail, lights, cx))
            })
            .collect();
        let title_tabs = h_flex()
            .debug_selector(|| "title-tabs".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(segments);
        h_flex()
            .debug_selector(|| "tab-row".into())
            .relative()
            .flex_none()
            .h(theme::TITLE_BAR_HEIGHT)
            .bg(colors.background)
            .child(strip_rule(cx).debug_selector(|| "title-rule".into()))
            .children(left_title)
            .child(title_tabs)
            .children(right_title)
    }

    /// `surface`'s top groups, left to right, each with its depth-first index, as `render`'s, and its width.
    fn top_rects(&self, surface: Surface) -> Vec<(usize, GroupId, f32)> {
        let top = self.layout.top_groups(surface);
        self.layout
            .rects(surface)
            .into_iter()
            .enumerate()
            .filter(|(_, (group, _))| top.contains(group))
            .map(|(k, (group, rect))| (k, group, rect.width))
            .collect()
    }

    /// Each end piece's element; spacers, not padding, so each segment's width is its group's share.
    fn render_ends(
        &self,
        pieces: [Option<EndPiece>; 2],
        lights: Pixels,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        pieces
            .into_iter()
            .flatten()
            .map(|piece| match piece {
                EndPiece::Lights => Self::render_drag_region("title-drag-start", cx)
                    .flex_none()
                    .w(lights)
                    .into_any_element(),
                EndPiece::Toggle(side) => strip_slot()
                    .child(self.render_dock_toggle(side, cx))
                    .into_any_element(),
                EndPiece::Inset => div().flex_none().w(theme::ROW_INSET).into_any_element(),
            })
            .collect()
    }

    /// A dock's top group `k`'s share of the row over it: `lead`, its view icons, a drag region, then `trail`.
    fn render_dock_segment(
        &self,
        side: Side,
        (k, group, width): (usize, GroupId, f32),
        (lead, trail): (Vec<AnyElement>, Vec<AnyElement>),
        cx: &mut Context<Self>,
    ) -> Div {
        let drag = match side {
            Side::Left => "title-drag-left",
            Side::Right => "title-drag-right",
        };
        h_flex()
            .flex_basis(px(0.))
            .flex_grow(width)
            .min_w_0()
            .h_full()
            .children(lead)
            .children(
                self.render_dock_strip(side, group, k, cx)
                    .map(|strip| strip_slot().child(strip)),
            )
            .child(Self::render_drag_region(ElementId::Name(selector(drag, k).into()), cx).flex_1())
            .children(trail)
    }

    /// Over `side`'s dock: each top group's icons between its end pieces (D2).
    fn render_title_dock(
        &self,
        side: Side,
        lights: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let surface = Surface::Dock(side);
        let top = self.top_rects(surface);
        let count = top.len();
        let segments: Vec<_> = top
            .into_iter()
            .enumerate()
            .map(|(ix, rect)| {
                let Ends { lead, trail } = self.title_ends(surface, ix, count);
                let ends = (
                    self.render_ends(lead, lights, cx),
                    self.render_ends(trail, lights, cx),
                );
                self.render_dock_segment(side, rect, ends, cx)
            })
            .collect();
        let colors = theme::current(cx).colors;
        let selector = format!("title-{}", name(side));
        let ruled = h_flex()
            .debug_selector(move || selector)
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(width(side))
            .h_full();
        match side {
            Side::Left => ruled.border_r(theme::RULE_WIDTH),
            Side::Right => ruled.border_l(theme::RULE_WIDTH),
        }
        .border_color(colors.border_variant)
        .children(segments)
    }

    /// A press arms the move; the root starts it on the next move, wherever the pointer is by then.
    fn render_drag_region(id: impl Into<ElementId>, cx: &mut Context<Self>) -> Stateful<Div> {
        identified(div(), id)
            .h_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _, window, _| {
                    // Keeps the editor's focus: the root would take it on the press (`div.rs:2765-2780`).
                    window.prevent_default();
                    shell.title_drag_armed = true;
                }),
            )
            .on_click(|event: &ClickEvent, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }

    /// On the root, as a fast drag leaves a region first; any move disarms, as a release outside the window reaches no listener.
    pub(super) fn title_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        let armed = std::mem::take(&mut self.title_drag_armed);
        if armed && event.pressed_button == Some(MouseButton::Left) {
            window.start_window_move();
        }
    }

    pub(super) fn title_drag_release(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.title_drag_armed = false;
    }
}

#[cfg(any(test, feature = "shot"))]
impl AppShell {
    /// The left dock toggle's centre, whether the dock shows or hides.
    pub(crate) fn left_toggle_point() -> gpui::Point<Pixels> {
        gpui::point(
            theme::TRAFFIC_LIGHT_PADDING + theme::ICON_BUTTON_WIDTH / 2.,
            theme::TITLE_BAR_HEIGHT / 2.,
        )
    }

    /// The centre of icon `index` in a dock's strip, in the title row.
    pub(crate) fn dock_icon_point(
        side: Side,
        index: usize,
        window: &Window,
    ) -> gpui::Point<Pixels> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count far below 2^24 converts to f32 exactly"
        )]
        let index = index as f32;
        let x = match side {
            Side::Left => theme::TRAFFIC_LIGHT_PADDING + theme::ICON_BUTTON_WIDTH,
            Side::Right => {
                window.viewport_size().width - super::RIGHT_DOCK_WIDTH
                    + theme::RULE_WIDTH
                    + theme::ROW_INSET
            }
        };
        gpui::point(
            x + theme::ICON_BUTTON_WIDTH * (index + 0.5),
            theme::TITLE_BAR_HEIGHT / 2.,
        )
    }
}
