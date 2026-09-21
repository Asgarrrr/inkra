//! The binding between files and buffers. Handles are **strong**: weak ones
//! dropped an unsaved buffer the moment the editor switched away, losing the
//! text and the undo history with no prompt (critical defect 2 in
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Context, Entity, Task, actions, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Vault};

actions!(vault, [Save]);

struct OpenNote {
    buffer: Entity<Buffer>,
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
}

pub(crate) struct NoteStore {
    fs: Arc<dyn Fs>,
    /// `None` when launched on a single file, or on nothing.
    vault: Option<Entity<Vault>>,
    open: HashMap<EntryId, OpenNote>,
    current: Option<EntryId>,
}

impl NoteStore {
    pub(crate) fn new(fs: Arc<dyn Fs>, vault: Option<Entity<Vault>>) -> Self {
        Self {
            fs,
            vault,
            open: HashMap::new(),
            current: None,
        }
    }

    pub(crate) fn vault(&self) -> Option<&Entity<Vault>> {
        self.vault.as_ref()
    }

    /// `#[cfg(test)]` because nothing in the UI asks, and an unread method
    /// fails `-D warnings`. The only way to assert the map kept a buffer.
    #[cfg(test)]
    pub(crate) fn buffer_for(&self, id: EntryId) -> Option<&Entity<Buffer>> {
        self.open.get(&id).map(|note| &note.buffer)
    }

    /// The note `save_current` would write. `#[cfg(test)]`, as above.
    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<EntryId> {
        self.current
    }

    /// A note that is not open has no buffer to differ from the file.
    pub(crate) fn is_modified(&self, id: EntryId, cx: &App) -> bool {
        self.open
            .get(&id)
            .is_some_and(|note| note.buffer.read(cx).version() != note.saved_version)
    }

    /// Buffers live for the session; eviction is deferred, with its trigger in
    /// the spec. Deliberately does **not** set `current`: this resolves on the
    /// load's schedule, so it would record whichever load landed last rather
    /// than the note on screen.
    pub(crate) fn open_note(
        &mut self,
        id: EntryId,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Buffer>>> {
        if let Some(note) = self.open.get(&id) {
            return Task::ready(Some(note.buffer.clone()));
        }

        let Some(vault) = self.vault.clone() else {
            return Task::ready(None);
        };
        let Some(path) = vault.read(cx).absolute(id) else {
            return Task::ready(None);
        };
        let fs = Arc::clone(&self.fs);

        cx.spawn(async move |this, cx| {
            let for_task = path.clone();
            let loaded = cx
                .background_spawn(async move { fs.load(&for_task).await })
                .await
                .ok()?;
            this.update(cx, |this, cx| {
                // Two clicks on one unopened note race. The second must not
                // replace a buffer the editor may already have been typed into.
                if let Some(note) = this.open.get(&id) {
                    return note.buffer.clone();
                }
                let buffer = cx.new(|_| Buffer::new(loaded.text));
                this.open.insert(
                    id,
                    OpenNote {
                        buffer: buffer.clone(),
                        path,
                        // `Buffer::version` starts at zero: a fresh note is clean.
                        saved_version: 0,
                    },
                );
                cx.notify();
                buffer
            })
            .ok()
        })
    }

    /// Records which note `save_current` writes, and runs after the bind, never
    /// when a load happens to land. `adopt` is the only other writer and keeps
    /// the rule by being synchronous. Ignores a note that is not open —
    /// `save_current` would silently pick nothing.
    pub(crate) fn set_current(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        self.current = Some(id);
        cx.notify();
    }

    /// Keyed by `EntryId::DETACHED`: the file was named on the command line,
    /// and no scanned entry can carry that id.
    pub(crate) fn adopt(&mut self, buffer: Entity<Buffer>, path: PathBuf) {
        self.open.insert(
            EntryId::DETACHED,
            OpenNote {
                buffer,
                path,
                saved_version: 0,
            },
        );
        self.current = Some(EntryId::DETACHED);
    }

    /// A no-op when nothing changed. The version is captured **with** the text,
    /// before the spawn, so keystrokes landing mid-write stay dirty.
    /// `finalize_transaction` runs here and never in an observer callback: a
    /// second lease of one entity panics
    /// (`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:231`).
    ///
    /// Deferred in the spec, with triggers: ordering overlapping writes to one
    /// note, and waiting for the write on quit.
    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(id) = self.current else {
            return Task::ready(());
        };
        let Some(note) = self.open.get(&id) else {
            return Task::ready(());
        };

        // `update`, never `as_mut`: the latter notifies on drop
        // (`gpui/src/app.rs:3280-3286`) and `finalize_transaction` changes no
        // text. The lease also ends with the closure, before the next one.
        let (text, version) = note.buffer.update(cx, |buffer, _| {
            buffer.finalize_transaction();
            (buffer.text().to_owned(), buffer.version())
        });
        if version == note.saved_version {
            return Task::ready(());
        }

        let fs = Arc::clone(&self.fs);
        let path = note.path.clone();
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn(async move { fs.save(&path, &text).await })
                .await;
            let saved = match written {
                Ok(saved) => saved,
                Err(error) => {
                    eprintln!("save failed: {error}");
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(note) = this.open.get_mut(&id) {
                    note.saved_version = version;
                }
                // The write replaced the file, so the entry's `(device, inode)`
                // moved. Re-keying here is what keeps the `EntryId` — and with
                // it every buffer in `open` — alive across the next rescan.
                if let Some(vault) = this.vault.clone() {
                    vault.update(cx, |vault, _| vault.reidentify(id, saved.identity));
                }
                cx.notify();
            });
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, prelude::*};
    use inkra_text::{Buffer, ByteOffset};
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{EntryId, Vault};

    use super::{Fs, NoteStore};
    use crate::editor::EditorView;
    use crate::keymap::key_bindings;
    use crate::shell::AppShell;

    fn open_two_notes(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (Entity<NoteStore>, EntryId, EntryId) {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        // By name, never by index: the comparator decides the order.
        let id_of = |name: &str| {
            scanned
                .entries()
                .find(|entry| entry.path.display_name() == name)
                .map(|entry| entry.id)
                .expect("entry exists")
        };
        let (a, b) = (id_of("a"), id_of("b"));
        let vault = cx.update(|cx| cx.new(|_| scanned));
        let store = cx.update(|cx| cx.new(|_| NoteStore::new(fs, Some(vault))));
        (store, a, b)
    }

    /// Neither load takes the already-open early return, so the second would
    /// overwrite the first's entry and the store would forget a live buffer.
    #[gpui::test]
    async fn two_loads_of_one_note_settle_on_one_buffer(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");

        let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let first = store.update(cx, |store, cx| store.open_note(a, cx));
        let second = store.update(cx, |store, cx| store.open_note(a, cx));
        let (first, second) = (first.await, second.await);

        let held = store.update(cx, |store, _| store.buffer_for(a).map(Entity::entity_id));
        assert!(held.is_some(), "the note is open");
        assert_eq!(first.map(|buffer| buffer.entity_id()), held);
        assert_eq!(second.map(|buffer| buffer.entity_id()), held);
    }

    /// The defect: weak handles meant switching notes dropped the last one, so
    /// returning reloaded from disk and lost the text and the undo history.
    #[gpui::test]
    fn switching_away_and_back_preserves_unsaved_text(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");

        let (store, a, b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        cx.run_until_parked();
        store.update(cx, |store, cx| {
            let buffer = store.buffer_for(a).cloned().expect("a is open");
            buffer.update(cx, |buffer, _| {
                buffer.transact_at(Instant::now(), |buffer| {
                    buffer.insert(ByteOffset(0), "UNSAVED ");
                });
            });
        });

        store.update(cx, |store, cx| store.open_note(b, cx).detach());
        cx.run_until_parked();
        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        cx.run_until_parked();

        store.update(cx, |store, cx| {
            let buffer = store.buffer_for(a).expect("the buffer must still exist");
            assert_eq!(buffer.read(cx).text(), "UNSAVED alpha");
            assert!(
                buffer.read(cx).version() != 0,
                "the undo history survived too"
            );
        });
    }

    /// `with_editor` and `dispatching` (`editor/tests.rs`) root the window at
    /// `EditorView`, so neither can prove an `AppShell`-scoped binding fires.
    fn open_shell(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> VisualTestContext {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("before")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
        let notes = cx.update(|cx| {
            cx.new(|_| {
                let mut store = NoteStore::new(fs, None);
                store.adopt(buffer, PathBuf::from("/vault/a.md"));
                store
            })
        });
        let window = cx.add_window(move |_, cx| AppShell::new(editor, notes, cx));
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();
        VisualTestContext::from_window(window.into(), cx)
    }

    /// Focus is in `EditorView`; the binding is scoped to `AppShell`. Proves
    /// the action dispatches up the tree rather than being swallowed.
    #[gpui::test]
    fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        // The caret opens at the end of the buffer (`EditorView::new`).
        visual.simulate_input("X");
        visual.simulate_keystrokes("cmd-s");
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX")
        );
    }

    /// A save changes no text, so no observer may fire: only `grouping_blocked`
    /// moved. Swapping `update` back to `as_mut` reintroduces this, because
    /// `as_mut` notifies unconditionally (`gpui/src/app.rs:3280-3286`).
    #[gpui::test]
    fn saving_does_not_report_the_buffer_as_changed(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("before")));
        let notes = cx.update(|cx| {
            cx.new(|_| {
                let mut store = NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None);
                store.adopt(buffer.clone(), PathBuf::from("/vault/a.md"));
                store
            })
        });
        // Dirty, so the save reaches the write rather than the early return.
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        });

        // Subscribed after the edit: that notify is legitimate.
        let notified = Arc::new(AtomicUsize::new(0));
        let _guard = cx.update(|cx| {
            let notified = Arc::clone(&notified);
            cx.observe(&buffer, move |_, _| {
                notified.fetch_add(1, Ordering::SeqCst);
            })
        });

        notes.update(cx, NoteStore::save_current).detach();
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("Xbefore"),
            "the save must still have happened"
        );
        assert_eq!(
            notified.load(Ordering::SeqCst),
            0,
            "a save changes no text, so nothing may observe one"
        );
    }

    /// The file holds text the buffer never had, so a write of any kind shows —
    /// putting the version comparison under test, not just a matching write.
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
