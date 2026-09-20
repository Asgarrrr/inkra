//! The window's root view: composes the sidebar and the editor.
//!
//! Named, opaque slots rather than an ad hoc `div` tree assembled at render
//! time — see `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`
//! for why, and what this deliberately leaves out (sidebar content, resize,
//! a status bar, multi-pane).

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, div, px, rgb,
};

use crate::editor::EditorView;
use crate::sidebar::Sidebar;
use crate::vault::{NoteStore, Save};

pub(crate) struct AppShell {
    editor: Entity<EditorView>,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
}

impl AppShell {
    /// `AppShell` composes the window; it does not decide what buffer opens in
    /// it.
    ///
    /// The sidebar is built here rather than passed in, for the same reason:
    /// it is part of the composition, and it needs nothing its two arguments do
    /// not already carry.
    pub(crate) fn new(
        editor: Entity<EditorView>,
        notes: Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), editor.clone(), cx));
        Self {
            editor,
            notes,
            sidebar,
        }
    }

    /// Detached rather than held: the shell outlives every save, and a `Task`
    /// dropped on the spot would cancel the write it just started.
    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes.update(cx, NoteStore::save_current).detach();
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here rather than on `EditorView`: the sidebar will be
            // focusable, and a binding scoped to the editor would do nothing
            // while a note row holds focus. The action still reaches this
            // handler from the editor, because dispatch walks from the focused
            // node up to the root.
            .key_context("AppShell")
            .on_action(cx.listener(Self::save))
            .flex()
            .flex_row()
            .size_full()
            .child(
                // flex_none (grow 0, shrink 0): never competes with the
                // editor for space.
                div()
                    .flex_none()
                    .w(px(180.))
                    .h_full()
                    .bg(rgb(0x181818))
                    .child(self.sidebar.clone()),
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
    use std::sync::Arc;

    use gpui::{Focusable, TestAppContext, VisualTestContext, prelude::*};
    use inkra_text::Buffer;
    use inkra_vault::mem_fs::MemFs;

    use super::{AppShell, EditorView, NoteStore};
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
        let notes = cx.update(|cx| cx.new(|_| NoteStore::new(Arc::new(MemFs::new()), None)));
        let window = cx.add_window(move |_, cx| AppShell::new(editor, notes, cx));
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
