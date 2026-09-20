//! The binding between files and buffers.
//!
//! Holds **strong** `Entity<Buffer>` handles. Weak ones would drop an unsaved
//! buffer the moment the editor switched away from it, losing the text and the
//! undo history with no prompt — see critical defect 2 in
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Context, Entity, Task, actions, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Vault};

actions!(vault, [Save]);

/// What is known about one open note's relationship to its file.
struct OpenNote {
    buffer: Entity<Buffer>,
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
}

pub(crate) struct NoteStore {
    fs: Arc<dyn Fs>,
    /// `None` when the app was launched on a single file, or on nothing: there
    /// is then no tree to show and no id to open a note by.
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

    /// What the store itself still holds for a note.
    ///
    /// `#[cfg(test)]` because that is the truth today: nothing in the UI asks
    /// the store for a buffer it did not just open, and a method only tests
    /// call still fails `-D warnings` in the binary. The regression test needs
    /// it — reading the buffer back out of the map is the only way to assert
    /// the map kept it.
    #[cfg(test)]
    pub(crate) fn buffer_for(&self, id: EntryId) -> Option<&Entity<Buffer>> {
        self.open.get(&id).map(|note| &note.buffer)
    }

    /// The note `save_current` would write.
    ///
    /// `#[cfg(test)]` for the same reason as `buffer_for`: nothing in the UI
    /// asks, and an unread method fails `-D warnings`. The tests that this
    /// exists for assert the thing the UI cannot — that `current` and the
    /// buffer bound to the editor are the same note.
    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<EntryId> {
        self.current
    }

    /// True when the note's buffer has moved past what was last written.
    ///
    /// A note that is not open has no buffer to differ from the file, so it is
    /// never modified.
    pub(crate) fn is_modified(&self, id: EntryId, cx: &App) -> bool {
        self.open
            .get(&id)
            .is_some_and(|note| note.buffer.read(cx).version() != note.saved_version)
    }

    /// Loads a note, or returns the buffer already open for it.
    ///
    /// The map holds a **strong** handle, so an unsaved buffer survives being
    /// switched away from. There is no close action in this slice, so buffers
    /// live for the session; eviction is deferred with its trigger in the
    /// design spec.
    ///
    /// Deliberately does **not** set `current`. This resolves on whichever
    /// schedule the load takes, and the editor is rebound on the caller's; a
    /// note recorded here would be the one whose load landed last rather than
    /// the one on screen. `set_current` is the single writer, called by the
    /// caller once it has actually bound the buffer.
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
                // Two clicks on one unopened note put two loads in flight.
                // Whichever lands second must not replace what the first one
                // already handed to the editor: that buffer may have been typed
                // into, and replacing it would strand the text exactly the way
                // a weak handle does.
                if let Some(note) = this.open.get(&id) {
                    return note.buffer.clone();
                }
                let buffer = cx.new(|_| Buffer::new(loaded.text));
                this.open.insert(
                    id,
                    OpenNote {
                        buffer: buffer.clone(),
                        path,
                        // A freshly loaded note is clean, and `Buffer::version`
                        // starts at zero, so nothing has moved past the file.
                        saved_version: 0,
                    },
                );
                cx.notify();
                buffer
            })
            .ok()
        })
    }

    /// Records which note `save_current` writes and the sidebar dots.
    ///
    /// The single writer of `current` for anything the sidebar opens, and it
    /// runs after the buffer is bound to the editor, never when a load happens
    /// to land. `adopt` is the one other writer, and it keeps the same rule:
    /// it is synchronous, and `main.rs` has already built the editor around
    /// that buffer by the time it runs.
    ///
    /// Ignores a note that is not open: nothing can be current that has no
    /// buffer, and `save_current` would silently pick nothing.
    pub(crate) fn set_current(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        self.current = Some(id);
        cx.notify();
    }

    /// Registers an already-loaded buffer as the current note.
    ///
    /// `saved_version` starts at `Buffer::version`'s own starting value, so a
    /// freshly loaded note is clean and `⌘S` no-ops until something is typed.
    ///
    /// The note is keyed by `EntryId::DETACHED` because there is no vault
    /// behind it: the file was named on the command line, not found by a scan.
    /// No scanned entry can carry that id, so the two cannot collide once the
    /// sidebar starts opening notes by their real ones.
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

        // `update`, never `as_mut`: the latter notifies the buffer's observers
        // when its `GpuiBorrow` drops (`gpui/src/app.rs:3280-3286`), and
        // `finalize_transaction` changes no text. Reporting an edit that did
        // not happen would lie to anything subscribing to real ones — the same
        // invariant `EditorView::update_state` keeps for caret moves.
        //
        // The lease also ends with the closure, so nothing below can take a
        // second one, which panics rather than failing to compile
        // (`gpui/app/entity_map.rs:231`).
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
        // Resolved by name, never by index: the comparator decides the order and
        // this test must not silently depend on it.
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

    /// The same loss as the test below, by a narrower route: both loads are in
    /// flight at once, so neither takes the already-open early return, and the
    /// second would otherwise overwrite the entry the first one made — leaving
    /// the editor holding a buffer the store no longer knows about.
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

    /// The defect: holding buffers weakly meant switching notes dropped the last
    /// strong handle, so returning reloaded from disk and the unsaved text and
    /// the whole undo history were gone, with no prompt.
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

    /// Saving changes no text, so the buffer's observers must not fire for it.
    /// `editor/mod.rs` states that invariant for caret moves and the reason
    /// holds here: an autosave or a vault reload subscribing to real edits
    /// would be told the text moved when only `grouping_blocked` did.
    ///
    /// The trap is that `Entity::as_mut` notifies unconditionally when its
    /// `GpuiBorrow` drops (`gpui/src/app.rs:3280-3286`), while `update` leaves
    /// the decision to the callee. Swapping the two back reintroduces this.
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

        // Subscribed after the edit: the edit itself is a legitimate notify,
        // and only what the save does is under test.
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

    /// The file starts out holding text the buffer never had, so a write of any
    /// kind would show — which is what makes the version comparison, and not
    /// just a matching write, the thing under test.
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
