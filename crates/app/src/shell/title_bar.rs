//! The tab row is the window's title bar: the traffic lights' space, the toggles, the tabs and the drag regions (shell chrome spec, S3).

use gpui::{
    ClickEvent, Context, Div, ElementId, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Stateful, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use inkra_layout::Side;

use super::AppShell;
use super::group::selector;
use crate::theme;
use crate::ui::{h_flex, identified, keep_focus};

impl AppShell {
    /// Toggles and tabs keep their own presses: only a leaf region moves the window (S3-D3).
    pub(super) fn render_tab_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let drawn = |side| self.panes.is_some() && self.layout.is_drawn(side);
        let (left_drawn, right_drawn) = (drawn(Side::Left), drawn(Side::Right));
        let (left_toggle, right_toggle) = self.render_dock_toggles(cx);
        // Full screen hides the native buttons.
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        let (left_title, leading_toggle) = if left_drawn {
            (
                Some(self.render_title_sidebar(lights, left_toggle, cx)),
                None,
            )
        } else {
            (None, Some(left_toggle))
        };
        let (right_title, trailing_toggle) = if right_drawn {
            (Some(self.render_title_backlinks(right_toggle, cx)), None)
        } else {
            (None, right_toggle)
        };
        let mut leading_toggle = leading_toggle;
        let mut trailing_toggle = trailing_toggle;
        let top = self.layout.top_groups();
        // `k` is the depth-first index, as `render`'s; `rects` is depth-first.
        let segments: Vec<_> = self
            .layout
            .rects()
            .into_iter()
            .enumerate()
            .filter(|(_, (group, _))| top.contains(group))
            .map(|(k, (group, rect))| {
                let last = top.last() == Some(&group);
                h_flex()
                    .id(("group-strip", group.raw()))
                    .debug_selector(move || selector("group-strip", k))
                    // Basis 0, no padding: the strip's width is its column's share of the centre.
                    .flex_basis(px(0.))
                    .flex_grow(rect.width)
                    .min_w_0()
                    .h_full()
                    .capture_any_mouse_down(cx.listener(move |shell, _, window, cx| {
                        shell.activate_group(group, window, cx);
                    }))
                    .when_some(leading_toggle.take(), |segment, toggle| {
                        segment
                            .child(
                                Self::render_drag_region("title-drag-start", cx)
                                    .flex_none()
                                    .w(lights),
                            )
                            .children(toggle)
                    })
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
                    .when(last, |segment| {
                        segment
                            .children(trailing_toggle.take())
                            .when(!right_drawn, |segment| {
                                segment.child(div().flex_none().w(theme::ROW_INSET))
                            })
                    })
            })
            .collect();
        let title_tabs = h_flex()
            .debug_selector(|| "title-tabs".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            // Under the tabs only, so the side panels run to the window's top; the active tab, painted later, breaks it (D3).
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .border_b(theme::RULE_WIDTH)
                    .border_color(colors.border_variant),
            )
            .children(segments);
        h_flex()
            .debug_selector(|| "tab-row".into())
            .relative()
            .flex_none()
            .h(theme::TITLE_BAR_HEIGHT)
            .bg(colors.background)
            .children(left_title)
            .child(title_tabs)
            .children(right_title)
    }

    /// Over the right dock: its view icons, a drag region, then its toggle at the window's right edge.
    fn render_title_backlinks<Toggle: IntoElement>(
        &self,
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let strip = self.render_dock_strip(Side::Right, cx);
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-right".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::RIGHT_DOCK_WIDTH)
            .h_full()
            .px(theme::ROW_INSET)
            .border_l(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .children(strip)
            .child(Self::render_drag_region("title-drag-right", cx).flex_1())
            .children(toggle)
    }

    /// Over the left dock: the lights, its toggle, then its view icons (D2).
    fn render_title_sidebar<Toggle: IntoElement>(
        &self,
        lights: Pixels,
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let strip = self.render_dock_strip(Side::Left, cx);
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-left".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::LEFT_DOCK_WIDTH)
            .h_full()
            .border_r(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .child(
                Self::render_drag_region("title-drag-start", cx)
                    .flex_none()
                    .w(lights),
            )
            .children(toggle)
            .children(strip)
            .child(Self::render_drag_region("title-drag-left", cx).flex_1())
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
        #[allow(clippy::cast_precision_loss)]
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
