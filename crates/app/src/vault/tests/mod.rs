use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, prelude::*};
use inkra_text::Buffer;
use inkra_vault::Fs;

use crate::editor::EditorView;
use crate::keymap::key_bindings;
use crate::shell::AppShell;

use super::NoteStore;

mod autosave;
mod gone;
mod lifecycle;
mod quit;
mod reload;
mod rescan;
mod scans;
mod sessions;
mod trash;
mod watch;

pub(super) fn open_shell(
    cx: &mut TestAppContext,
    fs: Arc<dyn Fs>,
) -> (VisualTestContext, Entity<NoteStore>) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    // Loaded, not fabricated: `adopt` needs the real stamp, or the first
    // write would see a mismatch and mint an unwanted conflict file.
    let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md")))
        .expect("every caller inserts /vault/a.md first");
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(loaded.text.clone())));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
    let notes = cx.update(|cx| {
        cx.new(|cx| {
            let mut store = NoteStore::new(fs, None, cx);
            store.adopt(buffer, PathBuf::from("/vault/a.md"), loaded.stamp, cx);
            store
        })
    });
    let window = cx.add_window({
        let notes = notes.clone();
        move |_, cx| AppShell::new(Some(editor), notes, cx)
    });
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();
    (VisualTestContext::from_window(window.into(), cx), notes)
}
