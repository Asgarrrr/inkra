//! The shell's layout: the title row over the sidebar, note column and backlinks, and the picker over them.

use gpui::{
    App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Render,
    Styled, Window, div,
};

use super::{AppShell, BACKLINKS_WIDTH, SIDEBAR_WIDTH};
use crate::theme;

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

    fn render_empty(&self) -> impl IntoElement {
        let lines = self.empty_lines();
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // Without this the window default shows through.
            .bg(theme::editor_background())
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(theme::placeholder_text())
            .children(lines.iter().map(|line| div().child(*line)))
    }

    /// Over the note showing while it is gone from disk (watcher spec, F9).
    fn render_gone_banner(&self, cx: &App) -> Option<impl IntoElement> {
        let showing = self.showing()?;
        self.notes.read(cx).is_gone(showing).then(|| {
            div()
                .debug_selector(|| "gone-banner".into())
                .flex_none()
                .px_4()
                .bg(theme::gone_banner_background())
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .text_color(theme::gone_banner_text())
                .child("Deleted from disk. Typing restores it.")
        })
    }
}

/// A side panel's column: fixed width, the body's full height.
fn side_column(selector: &'static str, width: Pixels) -> Div {
    div()
        .debug_selector(move || selector.into())
        .flex_none()
        .w(width)
        .h_full()
}

impl AppShell {
    /// Under the title row: the sidebar, the note column and the backlinks.
    fn render_columns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = self.panes.as_ref();
        div()
            .flex()
            .flex_row()
            .flex_1()
            // A definite width: else taffy measures the row's content, and a keystroke shapes the note twice.
            .w_full()
            // `min_h_0`: a flex child's minimum is its content, which would outgrow the window.
            .min_h_0()
            .children(panes.filter(|panes| panes.shown.sidebar).map(|panes| {
                side_column("sidebar-column", SIDEBAR_WIDTH)
                    .bg(theme::sidebar_background())
                    .child(panes.sidebar.clone())
            }))
            .child(
                div()
                    .debug_selector(|| "note-column".into())
                    .flex_1()
                    // `min_w_0`: else the note's content width would widen the column past the window.
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .children(self.render_gone_banner(cx))
                    // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
                    .child(div().flex_1().min_h_0().child(match self.editor() {
                        Some(editor) => editor.clone().into_any_element(),
                        None => self.render_empty().into_any_element(),
                    })),
            )
            // Shown for an empty tab too, so a switch does not move the note column by the panel's width.
            .children(panes.filter(|panes| panes.shown.backlinks).map(|panes| {
                side_column("backlinks-column", BACKLINKS_WIDTH).child(panes.backlinks.clone())
            }))
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
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_backlinks))
            .on_action(cx.listener(Self::quit))
            .on_mouse_move(cx.listener(Self::title_drag_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::title_drag_release))
            .flex()
            .flex_col()
            .size_full()
            .child(self.render_tab_row(window, cx))
            .child(self.render_columns(cx))
            .children(self.render_modal(cx))
    }
}
