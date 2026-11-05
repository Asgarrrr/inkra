//! The tab row is the window's title bar: the traffic lights' space, the toggles, the tabs and the drag regions (shell chrome spec, S3).

use gpui::{
    ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};
use inkra_layout::Side;

use super::AppShell;
use crate::sidebar::NewNote;
use crate::theme;
use crate::ui::{IconButton, IconName, h_flex, identified, keep_focus};
use crate::vault_search::SearchVault;

impl AppShell {
    /// Toggles and tabs keep their own presses: only a leaf region moves the window (S3-D3).
    pub(super) fn render_tab_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let docks = self.panes.as_ref().map(|panes| &panes.docks);
        let left_drawn = docks.is_some_and(|docks| docks.dock(Side::Left).is_drawn());
        let right_drawn = docks.is_some_and(|docks| docks.dock(Side::Right).is_drawn());
        let (left_toggle, right_toggle) = self.render_dock_toggles(cx);
        // Full screen hides the native buttons.
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        let (left_title, leading_toggle) = if left_drawn {
            (
                Some(Self::render_title_sidebar(lights, left_toggle, cx)),
                None,
            )
        } else {
            (None, Some(left_toggle))
        };
        let (right_title, trailing_toggle) = if right_drawn {
            (Some(Self::render_title_backlinks(right_toggle, cx)), None)
        } else {
            (None, right_toggle)
        };
        let title_tabs = h_flex()
            .debug_selector(|| "title-tabs".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(!right_drawn, |segment| segment.pr(theme::ROW_INSET))
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
            .when_some(leading_toggle, |segment, toggle| {
                segment
                    .child(
                        Self::render_drag_region("title-drag-start", cx)
                            .flex_none()
                            .w(lights),
                    )
                    .children(toggle)
            })
            .child(self.render_tab_bar(cx))
            .child(Self::render_new_tab(cx))
            .child(
                Self::render_drag_region("title-drag-end", cx)
                    .flex_1()
                    .min_w(theme::TITLE_DRAG_MIN_WIDTH),
            )
            .children(trailing_toggle);
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

    /// Over the right dock: a drag region, then its toggle at the window's right edge.
    fn render_title_backlinks<Toggle: IntoElement>(
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-right".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::RIGHT_DOCK_WIDTH)
            .h_full()
            .pr(theme::ROW_INSET)
            .border_l(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .child(Self::render_drag_region("title-drag-right", cx).flex_1())
            .children(toggle)
    }

    /// Over the left dock: the lights, the vault's buttons, then its toggle at its right edge (D2).
    fn render_title_sidebar<Toggle: IntoElement>(
        lights: Pixels,
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-left".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::LEFT_DOCK_WIDTH)
            .h_full()
            .pr(theme::ROW_INSET)
            .border_r(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .child(
                Self::render_drag_region("title-drag-start", cx)
                    .flex_none()
                    .w(lights),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_0p5()
                    .child(
                        IconButton::new("sidebar-search", IconName::Search, "Search vault")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.search_vault(&SearchVault, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("sidebar-new-note", IconName::PenLine, "New note")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_note(&NewNote, window, cx);
                            })),
                    ),
            )
            .child(Self::render_drag_region("title-drag-left", cx).flex_1())
            .children(toggle)
    }

    /// A press arms the move; the root starts it on the next move, wherever the pointer is by then.
    fn render_drag_region(selector: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        identified(div(), selector)
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
    /// The left dock toggle's centre. Valid while the left dock shows.
    pub(crate) fn left_toggle_point() -> gpui::Point<Pixels> {
        gpui::point(
            super::LEFT_DOCK_WIDTH
                - theme::RULE_WIDTH
                - theme::ROW_INSET
                - theme::ICON_BUTTON_WIDTH / 2.,
            theme::TITLE_BAR_HEIGHT / 2.,
        )
    }

    /// The centre of icon `index` in a dock's bar; for a one-view dock, a point on its title (index 0).
    pub(crate) fn dock_icon_point(
        side: Side,
        index: usize,
        window: &Window,
    ) -> gpui::Point<Pixels> {
        let start = match side {
            Side::Left => Pixels::ZERO,
            Side::Right => {
                window.viewport_size().width - super::RIGHT_DOCK_WIDTH + theme::RULE_WIDTH
            }
        };
        #[allow(clippy::cast_precision_loss)]
        let index = index as f32;
        gpui::point(
            start + theme::ROW_INSET + theme::ICON_BUTTON_WIDTH * (index + 0.5),
            theme::TITLE_BAR_HEIGHT + (theme::PANEL_HEADER_HEIGHT - theme::RULE_WIDTH) / 2.,
        )
    }
}
