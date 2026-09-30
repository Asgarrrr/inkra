//! The shell's layout: sidebar, note column, backlinks, and the picker over them.

use gpui::{
    App, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div,
};

use super::{AppShell, BACKLINKS_WIDTH, SIDEBAR_WIDTH};
use crate::theme;

impl AppShell {
    fn render_empty(&self, cx: &App) -> impl IntoElement {
        let lines: &[&str] = if self.notes.read(cx).vault().is_some() {
            &["Select a note."]
        } else {
            &[
                "No vault is open.",
                "Restart with a folder path to open one.",
            ]
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // Without this the window default shows through; check.sh renders nothing, so only a shot catches it.
            .bg(theme::editor_background())
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(theme::placeholder_text())
            .children(lines.iter().map(|line| div().child(*line)))
    }

    /// Over the note showing while it is gone from disk (watcher spec, F9).
    fn render_gone_banner(&self, cx: &App) -> Option<impl IntoElement> {
        let showing = self.showing?;
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
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .flex_none()
                    .w(SIDEBAR_WIDTH)
                    .h_full()
                    .bg(theme::sidebar_background())
                    .child(self.sidebar.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .children(self.render_gone_banner(cx))
                    // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
                    .child(div().flex_1().min_h_0().child(match &self.editor {
                        Some(editor) => editor.clone().into_any_element(),
                        None => self.render_empty(cx).into_any_element(),
                    })),
            )
            .children(self.editor.is_some().then(|| {
                div()
                    .flex_none()
                    .w(BACKLINKS_WIDTH)
                    .h_full()
                    .child(self.backlinks.clone())
            }))
            .children(self.render_modal(cx))
    }
}
