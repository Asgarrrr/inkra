//! The window's root view: composes the sidebar and the editor.
//!
//! Named, opaque slots rather than an ad hoc `div` tree assembled at render
//! time — see `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`
//! for why, and what this deliberately leaves out (sidebar content, resize,
//! a status bar, multi-pane).

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render, Styled,
    Window, div, px, rgb,
};

use crate::editor::EditorView;

pub(crate) struct AppShell {
    editor: Entity<EditorView>,
}

impl AppShell {
    /// Takes an already-built editor: `AppShell` composes the window, it
    /// does not decide what buffer opens in it.
    pub(crate) fn new(editor: Entity<EditorView>) -> Self {
        Self { editor }
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for AppShell {
    // Matches EditorView::render's own allow (editor/mod.rs): RRGGBB hex
    // literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .child(
                // flex_none (grow 0, shrink 0): never competes with the
                // editor for space.
                div().flex_none().w(px(180.)).h_full().bg(rgb(0x181818)),
            )
            .child(
                // flex_1 (grow 1, shrink 1, basis 0%) makes this wrapper
                // claim all remaining row width — gpui/styled.rs:181-186.
                // EditorView's own .size_full() then fills 100% of this
                // wrapper's resolved size; EditorView never needs to know
                // it's no longer the window root.
                div().flex_1().h_full().child(self.editor.clone()),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use gpui::{Focusable, TestAppContext, VisualTestContext, prelude::*};
    use inkra_text::Buffer;

    use super::{AppShell, EditorView};
    use crate::keymap::key_bindings;

    /// The one regression this file exists to guard: the existing
    /// `simulate_keystrokes` tests in `editor/tests.rs` open a window with
    /// `EditorView` as the direct root, so they never exercise `AppShell` at
    /// all. Only this test opens the window the way `main.rs` actually does
    /// — through `AppShell` — and proves a keystroke still reaches the
    /// editor once it is no longer the window root.
    #[gpui::test]
    fn a_keystroke_through_app_shell_reaches_the_editor(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
        let window = cx.add_window(move |_, _| AppShell::new(editor));
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_input("hi");

        window
            .update(&mut visual, |_, _, cx| {
                assert_eq!(buffer.read(cx).text(), "hi");
            })
            .unwrap();
    }
}
