//! The tab row is the window's title bar: the traffic lights' space, the toggles, the tabs and the drag regions (shell chrome spec, S3).

use gpui::{
    ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};

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
        let shown = self.panes.as_ref().map(|panes| panes.shown);
        let sidebar_shown = shown.is_some_and(|shown| shown.sidebar);
        let backlinks_shown = shown.is_some_and(|shown| shown.backlinks);
        let (sidebar_toggle, backlinks_toggle) = self.render_panel_toggles(cx).unzip();
        // Full screen hides the native buttons.
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        let (title_sidebar, leading_toggle) = if sidebar_shown {
            (
                Some(Self::render_title_sidebar(lights, sidebar_toggle, cx)),
                None,
            )
        } else {
            (None, Some(sidebar_toggle))
        };
        let (title_backlinks, trailing_toggle) = if backlinks_shown {
            (
                Some(Self::render_title_backlinks(backlinks_toggle, cx)),
                None,
            )
        } else {
            (None, backlinks_toggle)
        };
        let title_tabs = h_flex()
            .debug_selector(|| "title-tabs".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(!backlinks_shown, |segment| segment.pr(theme::ROW_INSET))
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
            .children(title_sidebar)
            .child(title_tabs)
            .children(title_backlinks)
    }

    /// Over the backlinks: a drag region, then their toggle at the window's right edge.
    fn render_title_backlinks<Toggle: IntoElement>(
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-backlinks".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::BACKLINKS_WIDTH)
            .h_full()
            .pr(theme::ROW_INSET)
            .border_l(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .child(Self::render_drag_region("title-drag-backlinks", cx).flex_1())
            .children(toggle)
    }

    /// Over the sidebar: the lights, its buttons, then its toggle at its right edge (D2).
    fn render_title_sidebar<Toggle: IntoElement>(
        lights: Pixels,
        toggle: Option<Toggle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<Toggle> {
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "title-sidebar".into())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .flex_none()
            .w(super::SIDEBAR_WIDTH)
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
            .child(Self::render_drag_region("title-drag-sidebar", cx).flex_1())
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
    /// The sidebar toggle's centre. Valid while the sidebar shows.
    pub(crate) fn sidebar_toggle_point() -> gpui::Point<Pixels> {
        gpui::point(
            super::SIDEBAR_WIDTH
                - theme::RULE_WIDTH
                - theme::ROW_INSET
                - theme::ICON_BUTTON_WIDTH / 2.,
            theme::TITLE_BAR_HEIGHT / 2.,
        )
    }
}
