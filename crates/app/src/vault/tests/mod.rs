use std::sync::Arc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, prelude::*};
use inkra_vault::Fs;

use crate::editor::EditorView;
use crate::keymap::key_bindings;
use crate::shell::{AppShell, Left};
use crate::test_support::detached_store;

use super::NoteStore;

mod autosave;
mod gone;
mod lifecycle;
mod moves;
mod quit;
mod reload;
mod rescan;
mod retry;
mod scans;
mod sessions;
mod trash;
mod watch;

pub(super) fn open_shell(
    cx: &mut TestAppContext,
    fs: Arc<dyn Fs>,
) -> (VisualTestContext, Entity<NoteStore>) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let (buffer, notes) = detached_store(cx, fs, "/vault/a.md");
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer, cx)));
    let window = cx.add_window({
        let notes = notes.clone();
        move |window, cx| {
            AppShell::new(Some(editor), notes, None, None, Left::default(), window, cx)
        }
    });
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();
    (VisualTestContext::from_window(window.into(), cx), notes)
}
