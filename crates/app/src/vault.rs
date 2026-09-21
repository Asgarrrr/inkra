//! The binding between files and buffers. Handles are **strong**: weak ones
//! dropped an unsaved buffer the moment the editor switched away, losing the
//! text and the undo history with no prompt (critical defect 2 in
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::channel::oneshot;
use gpui::{App, Context, Entity, EventEmitter, Subscription, Task, actions, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Vault};

actions!(vault, [Save]);

/// How long a note may stay dirty before it is written. Obsidian's cadence: the
/// write lands this long after typing *starts*, not this long after it stops.
/// An idle debounce — Zed's `AutosaveSetting::AfterDelay` — bounds nothing,
/// because a long uninterrupted writing session never reaches idle, and that is
/// the session whose loss hurts most.
///
/// A constant rather than a setting: this repo has no settings system, and
/// adding one to hold a single number is the wrong order.
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

/// A write failed. Emitted rather than printed so the deferred status line has
/// one seam to subscribe to instead of two call sites that grew apart — and so
/// the sidebar can tell a note whose last save failed from one being typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SaveFailed(pub(crate) EntryId);

impl EventEmitter<SaveFailed> for NoteStore {}

struct OpenNote {
    buffer: Entity<Buffer>,
    /// Where the note was when it was opened. Only `EntryId::DETACHED` reads
    /// it; every other note resolves its path against the vault at write time,
    /// because a rename moves it and nothing refreshes this.
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
    /// The pending write chain. Held, and never replaced by dropping it: a
    /// dropped `Task` never runs, so a queued write would vanish
    /// (`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`). Each
    /// new chain awaits this one instead, which is also what makes a note have
    /// exactly one writer.
    save: Option<Task<()>>,
    /// A chain is queued and has not reached its timer yet. Edits check this
    /// and return, so a run of typing keeps the first deadline rather than
    /// pushing it forward for as long as the user keeps going.
    scheduled: bool,
    /// Resolves the pending chain's timer early. `⌘S`, a note switch and the
    /// quit flush use it; an edit never does. Without it an explicit `⌘S`
    /// during a pending window would be absorbed by `scheduled` and write
    /// nothing, up to `AUTOSAVE_DELAY` late.
    cancel: Option<oneshot::Sender<()>>,
}

pub(crate) struct NoteStore {
    fs: Arc<dyn Fs>,
    /// `None` when launched on a single file, or on nothing.
    vault: Option<Entity<Vault>>,
    open: HashMap<EntryId, OpenNote>,
    current: Option<EntryId>,
    /// One per open note, for as long as it is open. A buffer with no observer
    /// never schedules a write, so a note edited and then switched away from
    /// would keep its changes only in memory.
    observers: HashMap<EntryId, Subscription>,
}

impl NoteStore {
    pub(crate) fn new(fs: Arc<dyn Fs>, vault: Option<Entity<Vault>>) -> Self {
        Self {
            fs,
            vault,
            open: HashMap::new(),
            current: None,
            observers: HashMap::new(),
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
                this.track(id, buffer.clone(), path, cx);
                cx.notify();
                buffer
            })
            .ok()
        })
    }

    /// Records which note `⌘S` writes, and runs after the bind, never when a
    /// load happens to land. `adopt` is the only other writer and keeps the
    /// rule by being synchronous. Ignores a note that is not open — `⌘S` would
    /// silently pick nothing.
    ///
    /// Writes the note being left before recording the new one. Being the sole
    /// writer of `current` makes this the only place that can tell a switch
    /// happened at all.
    pub(crate) fn set_current(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        if let Some(leaving) = self.current.filter(|leaving| *leaving != id) {
            self.schedule_save(leaving, Duration::ZERO, cx);
        }
        self.current = Some(id);
        cx.notify();
    }

    /// Keyed by `EntryId::DETACHED`: the file was named on the command line,
    /// and no scanned entry can carry that id.
    pub(crate) fn adopt(&mut self, buffer: Entity<Buffer>, path: PathBuf, cx: &mut Context<Self>) {
        self.track(EntryId::DETACHED, buffer, path, cx);
        self.current = Some(EntryId::DETACHED);
    }

    /// Binds a buffer to a file and starts watching it. The observer is the
    /// whole autosave trigger: it fires on every edit and asks for a write.
    ///
    /// It must not lease the buffer it observes — a second lease of one entity
    /// panics (`gpui/app/entity_map.rs:231`) — which is why it only schedules.
    fn track(
        &mut self,
        id: EntryId,
        buffer: Entity<Buffer>,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let observer = cx.observe(&buffer, move |this, _buffer, cx| {
            this.schedule_save(id, AUTOSAVE_DELAY, cx);
        });
        self.observers.insert(id, observer);
        self.open.insert(
            id,
            OpenNote {
                buffer,
                path,
                // `Buffer::version` starts at zero: a fresh note is clean.
                saved_version: 0,
                save: None,
                scheduled: false,
                cancel: None,
            },
        );
    }

    /// Queues a write of one note, `delay` from now.
    ///
    /// `Duration::ZERO` preempts: it resolves any pending timer so the write
    /// happens in this turn rather than queueing behind it. Anything longer is
    /// absorbed when a chain is already pending, which is what makes the cadence
    /// periodic — a run of typing keeps the deadline it first set instead of
    /// pushing it forward until the user stops.
    fn schedule_save(&mut self, id: EntryId, delay: Duration, cx: &mut Context<Self>) {
        let Some(note) = self.open.get_mut(&id) else {
            return;
        };
        if delay.is_zero() {
            // Errs when the chain is already past its timer, which is exactly
            // the case where there is nothing left to preempt.
            if let Some(cancel) = note.cancel.take() {
                let _ = cancel.send(());
            }
        } else if note.scheduled {
            return;
        }

        let (cancel, cancelled) = oneshot::channel();
        note.cancel = Some(cancel);
        note.scheduled = !delay.is_zero();
        // Taken, not dropped: the new chain awaits it below. Replacing the slot
        // outright would cancel a write that has already been queued.
        let previous = note.save.take();

        note.save = Some(cx.spawn(async move |this, cx| {
            let timer = cx.background_executor().timer(delay).fuse();
            if let Some(previous) = previous {
                previous.await;
            }
            futures::select_biased! {
                // Only a deliberate send cancels. `Err` means the sender was
                // dropped because a later `schedule_save` replaced it, and
                // treating that as a cancel is how the gesture that promises
                // "write now" ends up writing nothing: `⌘S` installs a chain,
                // the next keystroke replaces the sender, and the chain returns
                // before reaching its ready timer.
                cancelled = cancelled.fuse() => if cancelled.is_ok() { return },
                () = timer.fuse() => {}
            }
            // Reopened before the write, not after: an edit landing while this
            // one is in flight must be able to queue the next write rather than
            // be swallowed and lost until the following keystroke.
            let reopened = this.update(cx, |this, _| {
                if let Some(note) = this.open.get_mut(&id) {
                    note.scheduled = false;
                }
            });
            if reopened.is_err() {
                return;
            }
            Self::write(id, this, cx).await;
        }));
    }

    /// `⌘S`. Preempts whatever the periodic chain had queued, so the write
    /// lands in this turn rather than up to `AUTOSAVE_DELAY` later.
    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current {
            self.schedule_save(id, Duration::ZERO, cx);
        }
    }

    /// Writes one note, once. A no-op when nothing changed.
    ///
    /// The version is captured **with** the text, before the spawn, so
    /// keystrokes landing mid-write stay dirty and schedule the next chain.
    /// `finalize_transaction` runs here and never in an observer callback: a
    /// second lease of one entity panics
    /// (`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:231`).
    ///
    /// Reached only from a `schedule_save` chain, which is what gives a note
    /// exactly one writer at a time.
    async fn write(id: EntryId, this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp) {
        let Ok(Some((fs, path, text, version))) = this.update(cx, |this, cx| {
            let note = this.open.get(&id)?;
            // `update`, never `as_mut`: the latter notifies on drop
            // (`gpui/src/app.rs:3280-3286`) and `finalize_transaction` changes
            // no text. The lease also ends with the closure, before the next.
            let (text, version) = note.buffer.update(cx, |buffer, _| {
                buffer.finalize_transaction();
                (buffer.text().to_owned(), buffer.version())
            });
            if version == note.saved_version {
                return None;
            }
            // Resolved against the vault rather than read from `OpenNote.path`:
            // a rename moves the entry and nothing refreshes that field, so the
            // cached path would recreate the note where it used to be. A
            // vaulted note whose entry is gone writes nothing rather than
            // falling back and recreating a file the vault no longer lists.
            let path = if id == EntryId::DETACHED {
                note.path.clone()
            } else {
                this.vault.as_ref()?.read(cx).absolute(id)?
            };
            Some((Arc::clone(&this.fs), path, text, version))
        }) else {
            return;
        };

        let written = cx
            .background_spawn(async move { fs.save(&path, &text).await })
            .await;
        let _ = this.update(cx, |this, cx| {
            let Ok(saved) = written else {
                // The note keeps its old `saved_version`, so it stays dirty and
                // the next edit schedules another attempt.
                cx.emit(SaveFailed(id));
                return;
            };
            if let Some(note) = this.open.get_mut(&id) {
                note.saved_version = version;
            }
            // The write replaced the file, so the entry's `(device, inode)`
            // moved. Re-keying here is what keeps the `EntryId` — and with it
            // every buffer in `open` — alive across the next rescan.
            if let Some(vault) = this.vault.clone() {
                vault.update(cx, |vault, _| vault.reidentify(id, saved.identity));
            }
            cx.notify();
        });
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

    use super::{AUTOSAVE_DELAY, Fs, NoteStore};
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
            cx.new(|cx| {
                let mut store = NoteStore::new(fs, None);
                store.adopt(buffer, PathBuf::from("/vault/a.md"), cx);
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
            cx.new(|cx| {
                let mut store = NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None);
                store.adopt(buffer.clone(), PathBuf::from("/vault/a.md"), cx);
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

        notes.update(cx, NoteStore::save_current);
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

    /// The cadence itself: typing schedules a write, and the clock delivers it
    /// with no keystroke from the user.
    #[gpui::test]
    fn typing_writes_the_note_without_cmd_s(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX")
        );
    }

    /// The write is scheduled, not immediate. Without this the periodic chain
    /// could be replaced by writing on every keystroke and nothing would catch
    /// it — a vault on a synced folder would pay for that silently.
    #[gpui::test]
    fn typing_does_not_write_before_the_delay_elapses(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("before"),
            "the note is dirty, but its deadline has not arrived"
        );
    }

    /// The property an idle debounce does not have. Each round types again
    /// *before* the previous deadline, which under Zed's `AfterDelay` would
    /// push the write forward forever; here the first deadline stands, so the
    /// file keeps up with the buffer one interval at a time.
    #[gpui::test]
    fn continuous_typing_writes_once_per_interval(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let mut expected = "before".to_owned();
        for round in 1..=3 {
            // The second keystroke lands inside the pending window. Under an
            // idle debounce it would push the deadline past the advance below
            // and this round would write nothing.
            visual.simulate_input("X");
            cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
            visual.simulate_input("Y");
            cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
            cx.run_until_parked();

            expected.push_str("XY");
            assert_eq!(
                fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
                Some(expected.as_str()),
                "round {round} must have reached disk"
            );
        }
    }

    /// The defect the review found in the design: `⌘S` routed through the same
    /// queue was absorbed by the pending-chain guard and wrote nothing until
    /// the periodic deadline arrived. The clock never advances here, so only a
    /// preempting `⌘S` can produce the write.
    #[gpui::test]
    fn cmd_s_preempts_a_pending_autosave(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let mut visual = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        // Schedules the periodic chain; its deadline is one full delay away.
        visual.simulate_input("X");
        visual.simulate_keystrokes("cmd-s");
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX"),
            "cmd-s must not wait for the deadline it preempted"
        );
    }

    /// A note left behind is written, so its edits do not sit in memory until
    /// the user happens to come back to it.
    #[gpui::test]
    fn switching_notes_writes_the_one_being_left(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        store.update(cx, |store, cx| store.open_note(b, cx).detach());
        cx.run_until_parked();
        store.update(cx, |store, cx| store.set_current(a, cx));
        store.update(cx, |store, cx| {
            let buffer = store.buffer_for(a).cloned().expect("a is open");
            buffer.update(cx, |buffer, _| {
                buffer.transact_at(Instant::now(), |buffer| {
                    buffer.insert(ByteOffset(0), "EDITED ");
                });
            });
        });

        store.update(cx, |store, cx| store.set_current(b, cx));
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("EDITED alpha"),
            "the outgoing note is written, with no clock advance"
        );
    }

    /// The wiring that makes an atomic save survivable, pinned at the level it
    /// actually runs. The `vault` crate proves `reidentify` works when called;
    /// only this proves the write path calls it. Nothing else in the `app`
    /// suite rescans, so without this the call can be deleted and the whole
    /// workspace stays green — until the first post-startup `rescan` caller
    /// lands and orphans the open buffer.
    #[gpui::test]
    fn a_save_keeps_the_note_reachable_across_a_rescan(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        cx.run_until_parked();
        let opened = store.update(cx, |store, _| store.buffer_for(a).map(Entity::entity_id));

        store.update(cx, |store, cx| {
            let buffer = store.buffer_for(a).cloned().expect("a is open");
            buffer.update(cx, |buffer, _| {
                buffer.transact_at(Instant::now(), |buffer| {
                    buffer.insert(ByteOffset(0), "X");
                });
            });
        });
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        // What create/rename/delete will do on every mutation, next slice.
        let vault = store.update(cx, |store, _| store.vault().cloned().expect("a vault"));
        vault.update(cx, |vault, _| {
            futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
        });

        vault.update(cx, |vault, _| {
            assert!(
                vault.entry(a).is_some(),
                "the save moved the inode; without the re-key this id names nothing"
            );
        });
        let reopened = store.update(cx, |store, cx| store.open_note(a, cx));
        assert_eq!(
            futures::executor::block_on(reopened).map(|buffer| buffer.entity_id()),
            opened,
            "the row still opens the buffer that holds the edit"
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
