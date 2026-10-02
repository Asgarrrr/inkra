//! The tab row is the window's title bar: the traffic lights' space, the toggles, the tabs and the drag regions (shell chrome spec, S3).

use gpui::{
    ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window, div,
};

use super::AppShell;
use crate::theme;
use crate::ui::{h_flex, identified};

impl AppShell {
    /// Toggles and tabs keep their own presses: only a leaf region moves the window (S3-D3).
    pub(super) fn render_tab_row(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (sidebar, backlinks) = self.render_panel_toggles(cx).unzip();
        // Full screen hides the native buttons.
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        h_flex()
            .debug_selector(|| "tab-row".into())
            .flex_none()
            .h(theme::TITLE_BAR_HEIGHT)
            .pr_1()
            .bg(theme::tab_bar_background())
            .border_b(theme::RULE_WIDTH)
            .border_color(theme::tab_rule())
            .child(
                Self::render_drag_region("title-drag-start", cx)
                    .flex_none()
                    .w(lights),
            )
            .children(sidebar)
            .child(self.render_tab_bar(cx))
            .child(
                Self::render_drag_region("title-drag-end", cx)
                    .flex_1()
                    .min_w(theme::TITLE_DRAG_MIN_WIDTH),
            )
            .children(backlinks)
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
