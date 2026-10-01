//! The shell's layout: sidebar, note column, backlinks, and the picker over them.

use gpui::{
    App, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div,
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

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .flex_none()
                    .w(SIDEBAR_WIDTH)
                    .h_full()
                    .bg(theme::sidebar_background())
                    .children(self.panes.as_ref().map(|panes| panes.sidebar.clone())),
            )
            .child(
                div()
                    .flex_1()
                    // `min_w_0`: else the tab bar's content width would widen the column past the window.
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    // A sibling of the editor's slot, outside its scroll `div` (T7).
                    .child(self.render_tab_bar(cx))
                    .children(self.render_gone_banner(cx))
                    // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
                    .child(div().flex_1().min_h_0().child(match self.editor() {
                        Some(editor) => editor.clone().into_any_element(),
                        None => self.render_empty().into_any_element(),
                    })),
            )
            // Shown for an empty tab too, so a switch does not move the bar by the column's width.
            .children(self.panes.as_ref().map(|panes| {
                div()
                    .debug_selector(|| "backlinks-column".into())
                    .flex_none()
                    .w(BACKLINKS_WIDTH)
                    .h_full()
                    .child(panes.backlinks.clone())
            }))
            .children(self.render_modal(cx))
    }
}
