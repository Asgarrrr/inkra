//! The shell's layout: the title row over the docks and the note column, and the picker over them.

use gpui::{
    App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled,
    Window, div, prelude::FluentBuilder,
};
use inkra_layout::Side;

use super::{AppShell, GoBack, GoForward};
use crate::theme;
use crate::ui::{Color, IconButton, IconName, h_flex, keep_focus};

impl AppShell {
    /// None while the restore is pending, so the launch does not flash the empty state before the note.
    pub(super) fn empty_lines(&self) -> &'static [&'static str] {
        if self.panes.is_none() {
            &[
                "No vault is open.",
                "Restart with a folder path to open one.",
            ]
        } else if self.restore_pending() {
            &[]
        } else {
            &["Select a note."]
        }
    }

    fn render_empty(&self, cx: &App) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let lines = self.empty_lines();
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // Without this the window default shows through.
            .bg(colors.editor_background)
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(Color::Placeholder.rgba(theme::current(cx)))
            .children(lines.iter().map(|line| div().child(*line)))
    }

    /// Over the note showing while it is gone from disk (watcher spec, F9).
    fn render_gone_banner(&self, cx: &App) -> Option<impl IntoElement> {
        let showing = self.showing()?;
        let colors = theme::current(cx).colors;
        self.notes.read(cx).is_gone(showing).then(|| {
            div()
                .debug_selector(|| "gone-banner".into())
                .flex_none()
                .px_4()
                .bg(colors.warning_background)
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .text_color(Color::Warning.rgba(theme::current(cx)))
                .child("Deleted from disk. Typing restores it.")
        })
    }

    /// Atop the note column, out of the editor's scroll; each arrow is lit when its walk would arrive (D4).
    fn render_nav_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let showing = self.active_note();
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(|| "nav-row".into())
            .flex_none()
            .h(theme::NAV_ROW_HEIGHT)
            .pl_3()
            .bg(colors.editor_background)
            // A press on the strip beside the arrows leaves the editor focused.
            .on_mouse_down(MouseButton::Left, keep_focus)
            .child(
                IconButton::new("nav-back", IconName::ArrowLeft, "Go back")
                    .disabled(!self.tabs.can_go_back(showing))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.go_back(&GoBack, window, cx);
                    })),
            )
            .child(
                IconButton::new("nav-forward", IconName::ArrowRight, "Go forward")
                    .disabled(!self.tabs.can_go_forward(showing))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.go_forward(&GoForward, window, cx);
                    })),
            )
    }
}

impl AppShell {
    /// Under the title row: the left dock, the note column and the right dock.
    fn render_columns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_1()
            // A definite width: else taffy measures the row's content, and a keystroke shapes the note twice.
            .w_full()
            // `min_h_0`: a flex child's minimum is its content, which would outgrow the window.
            .min_h_0()
            .children(self.render_dock(Side::Left, cx))
            .child(
                div()
                    .debug_selector(|| "note-column".into())
                    .flex_1()
                    // `min_w_0`: else the note's content width would widen the column past the window.
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_nav_row(cx))
                    .children(self.render_gone_banner(cx))
                    .children(self.render_notice(cx))
                    // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
                    .child(div().flex_1().min_h_0().child(match self.editor() {
                        Some(editor) => editor.clone().into_any_element(),
                        None => self.render_empty(cx).into_any_element(),
                    })),
            )
            // Shown for an empty tab too, so a switch does not move the note column by the dock's width.
            .children(self.render_dock(Side::Right, cx))
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here, not on `EditorView`: a note row can hold focus too, and dispatch walks up to the root.
            .key_context("AppShell")
            // Load-bearing: with no editor, focus lives on `self.focus_handle`; untracked, `cmd-s` is unreachable (gpui/src/window.rs:6244-6251).
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::search_vault))
            .on_action(cx.listener(Self::command_palette))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::close_active_tab))
            .on_action(cx.listener(Self::reopen_closed_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::activate_nth_tab))
            .on_action(cx.listener(Self::activate_last_tab))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(cx.listener(Self::toggle_left_dock))
            .on_action(cx.listener(Self::toggle_right_dock))
            .on_action(cx.listener(Self::select_theme))
            .on_action(cx.listener(Self::quit))
            .on_mouse_move(cx.listener(Self::title_drag_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::title_drag_release))
            .flex()
            .flex_col()
            .size_full()
            .when_some(self.backdrop, Styled::bg)
            .child(self.render_tab_row(window, cx))
            .child(self.render_columns(cx))
            .children(self.render_modal(cx))
    }
}
