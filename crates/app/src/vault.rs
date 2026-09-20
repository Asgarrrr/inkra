//! The binding between files and buffers.
//!
//! Holds **strong** `Entity<Buffer>` handles. Weak ones would drop an unsaved
//! buffer the moment the editor switched away from it, losing the text and the
//! undo history with no prompt — see critical defect 2 in
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Context, Entity, Task, actions, prelude::*};
use inkra_text::Buffer;
use inkra_vault::Fs;

actions!(vault, [Save]);

/// Which open note a buffer belongs to.
///
/// A placeholder: the vault model lands in a later slice and replaces this
/// with an id derived from the file's `(device, inode)` pair, so that a
/// renamed note keeps its identity. Until then ids are handed out in sequence
/// by [`NoteStore::adopt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EntryId(u32);

/// What is known about one open note's relationship to its file.
struct OpenNote {
    buffer: Entity<Buffer>,
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
}

pub(crate) struct NoteStore {
    fs: Arc<dyn Fs>,
    open: HashMap<EntryId, OpenNote>,
    current: Option<EntryId>,
    next_id: u32,
}

impl NoteStore {
    pub(crate) fn new(fs: Arc<dyn Fs>) -> Self {
        Self {
            fs,
            open: HashMap::new(),
            current: None,
            next_id: 0,
        }
    }

    /// Registers an already-loaded buffer as the current note.
    ///
    /// The only way a note becomes current in this slice: the sidebar, which
    /// loads a note on click, lands later. `saved_version` starts at
    /// `Buffer::version`'s own starting value, so a freshly loaded note is
    /// clean and `⌘S` no-ops until something is typed.
    pub(crate) fn adopt(&mut self, buffer: Entity<Buffer>, path: PathBuf) {
        let id = EntryId(self.next_id);
        self.next_id += 1;
        self.open.insert(
            id,
            OpenNote {
                buffer,
                path,
                saved_version: 0,
            },
        );
        self.current = Some(id);
    }

    /// Writes the current note. A no-op when nothing has changed since the
    /// last write.
    ///
    /// The version is captured **with** the text, before the task is spawned.
    /// `⌘S` is async, so keystrokes can land while the write is in flight;
    /// recording the captured version rather than a later one leaves those
    /// edits correctly dirty.
    ///
    /// `finalize_transaction` runs here, in the action handler, and never in an
    /// observer callback: a second lease of one entity panics
    /// (`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:231`).
    ///
    /// Two things this deliberately does not do, both under Deferred in the
    /// design spec with their triggers: it does not order overlapping writes to
    /// one note, and nothing waits for the write on quit.
    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(id) = self.current else {
            return Task::ready(());
        };
        let Some(note) = self.open.get(&id) else {
            return Task::ready(());
        };

        // Scoped so the `GpuiBorrow` lease ends before anything else touches
        // the app: a second lease of one entity panics rather than failing to
        // compile (`gpui/app/entity_map.rs:231`).
        let (text, version) = {
            let mut buffer = note.buffer.as_mut(cx);
            buffer.finalize_transaction();
            (buffer.text().to_owned(), buffer.version())
        };
        if version == note.saved_version {
            return Task::ready(());
        }

        let fs = Arc::clone(&self.fs);
        let path = note.path.clone();
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn(async move { fs.save(&path, &text).await })
                .await;
            if let Err(error) = written {
                eprintln!("save failed: {error}");
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(note) = this.open.get_mut(&id) {
                    note.saved_version = version;
                }
                cx.notify();
            });
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gpui::{Focusable, TestAppContext, VisualTestContext, prelude::*};
    use inkra_text::Buffer;
    use inkra_vault::mem_fs::MemFs;

    use super::{Fs, NoteStore};
    use crate::editor::EditorView;
    use crate::keymap::key_bindings;
    use crate::shell::AppShell;

    /// Opens the window through `AppShell`, the way `main.rs` does.
    ///
    /// Neither `with_editor` nor `dispatching` (`crates/app/src/editor/tests.rs`)
    /// does: both make `EditorView` the window root, so neither can prove a
    /// binding scoped to the `AppShell` key context ever fires.
    fn open_shell(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> VisualTestContext {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("before")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
        let notes = cx.update(|cx| {
            cx.new(|_| {
                let mut store = NoteStore::new(fs);
                store.adopt(buffer, PathBuf::from("/vault/a.md"));
                store
            })
        });
        let window = cx.add_window(move |_, _| AppShell::new(editor, notes));
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();
        VisualTestContext::from_window(window.into(), cx)
    }

    /// Focus sits in the editor, whose own key context is `EditorView`. The
    /// binding is scoped to `AppShell`, so this is what proves the action still
    /// dispatches up the tree to the shell's `on_action` rather than being
    /// swallowed by the focused node's context.
    #[gpui::test]
    fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        // The caret opens at the end of the buffer (`EditorView::new`), so the
        // typed character lands after the loaded text.
        visual.simulate_input("X");
        visual.simulate_keystrokes("cmd-s");
        // The write runs on a background task; without this the assertion races it.
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX")
        );
    }

    /// The save is a no-op when nothing moved, so an untouched note is never
    /// rewritten — and the version comparison that decides it is exercised.
    #[gpui::test]
    fn cmd_s_on_an_untouched_note_leaves_the_file_alone(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "on disk, not in the buffer");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_keystrokes("cmd-s");
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("on disk, not in the buffer")
        );
    }
}
