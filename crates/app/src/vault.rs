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
use inkra_vault::{EntryId, Fs, Saved, Stamp, Vault};

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

/// A write found the file changed underneath it and preserved the other
/// version beside the note. Emitted for the same reason as `SaveFailed`: the
/// deferred status line gets one seam to subscribe to rather than two that grew
/// apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictPreserved {
    pub(crate) id: EntryId,
    pub(crate) path: PathBuf,
}

impl EventEmitter<ConflictPreserved> for NoteStore {}

/// Serialises one note's writes, and drops one that a newer write overtook.
///
/// One note normally has exactly one writer: `schedule_save` chains every write
/// behind the previous one. The quit flush is the exception, and cannot be
/// chained — `App::shutdown` blocks the foreground session while it polls the
/// quit futures (`gpui/src/app.rs:1073`,
/// `scheduler/src/test_scheduler.rs:100`, `:362`), so a quit write that awaited
/// the `cx.spawn` chain would wait for a task that can never resume. It
/// therefore issues its own write, and for a moment a note has two.
///
/// Dropping `NoteStore` does not settle this. `windows.clear()` releases the
/// store before the quit futures are polled, which cancels the chain's `Task`
/// — but cancelling a `Task` only stops it being polled *again*
/// (`async-task/src/task.rs:440-445`), and `RealFs::save` has no `.await` in
/// it, so a write a background thread already began runs to its `rename()`
/// regardless. The loser of that race is whichever `rename()` lands last, and
/// without ordering that can be the *older* text.
///
/// The lock alone cannot order them: `futures::lock::Mutex` documents no
/// fairness at all (`futures-util-0.3.34/src/lock/mutex.rs:14-21`), so
/// acquisition order is not issue order. The sequence is what decides; the lock
/// only stops the two `rename()`s interleaving.
struct Writer {
    /// The newest write that reached the filesystem, and what it left behind.
    ///
    /// Locked and unlocked entirely inside a background task. No foreground
    /// code may hold this guard: the quit future is polled while the foreground
    /// session is blocked, so a guard held by a `cx.spawn` chain would never be
    /// released and shutdown would spin until its timeout.
    state: futures::lock::Mutex<WriterState>,
}

/// The expectation lives here, and not beside `saved_version` on `OpenNote`,
/// because a write captures its text under `&mut App` and only *then* waits for
/// the lock. A second write issued while the first is in flight would therefore
/// carry a stamp the first is about to invalidate, find a file it did not
/// recognise — one it had just written itself — and preserve our own new text
/// into a conflict file. Ordinary typing would mint junk beside every note.
///
/// Read after the lock, the expectation is always what our own last write
/// actually produced, or what the load returned before there was one.
struct WriterState {
    /// The sequence of the newest write that reached the filesystem.
    written: u64,
    /// What the file looked like after that write.
    stamp: Stamp,
}

/// Writes one note's text, unless a newer write already landed.
///
/// `None` means exactly that — overtaken, not failed. The caller must not
/// report it as a failure, and must not advance `saved_version`: the text on
/// disk is the *other* write's, not this one's.
///
/// Free rather than a method because both writers call it from a background
/// task, where no `App` is reachable (`gpui/src/app.rs:1004-1013` holds the
/// `RefCell` across the whole of `shutdown`).
async fn write_through(
    writer: Arc<Writer>,
    fs: Arc<dyn Fs>,
    path: PathBuf,
    text: String,
    seq: u64,
) -> Option<inkra_vault::Result<Saved>> {
    let mut state = writer.state.lock().await;
    if seq < state.written {
        return None;
    }
    let result = fs.save(&path, &text, state.stamp).await;
    // Only a write that landed advances the mark, so the field keeps meaning
    // what it says. Not covered by a test, and deliberately so: sequences are
    // minted monotonically and used once, so a retry always carries a higher
    // number than the failure before it, and no reachable interleaving can tell
    // the two versions apart. The guard is here to keep the invariant true for
    // the next reader of `written`, not because a test demands it.
    if let Ok(saved) = &result {
        state.written = seq;
        state.stamp = saved.stamp;
    }
    Some(result)
}

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
    /// Shared with every write of this note, and with the quit flush. See
    /// `Writer`.
    writer: Arc<Writer>,
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
    /// Stamps every write with its issue order. Minted where the text is
    /// captured, under `&mut App`, so the order is the order the writes were
    /// decided — which `Buffer::version` cannot give: an undo *lowers* the
    /// version (`text/src/buffer.rs:114-119`), so a flush after an undo would
    /// carry a lower number than the write it has to beat.
    next_write: u64,
    /// Unsubscribes the quit flush when the store goes. Never read; holding it
    /// is the whole point, because dropping a `Subscription` cancels it.
    _quit: Subscription,
}

impl NoteStore {
    pub(crate) fn new(
        fs: Arc<dyn Fs>,
        vault: Option<Entity<Vault>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            fs,
            vault,
            open: HashMap::new(),
            current: None,
            observers: HashMap::new(),
            next_write: 0,
            _quit: cx.on_app_quit(Self::flush_all),
        }
    }

    /// Issue order for one write. Wraps only after 2^64 writes, which at the
    /// autosave cadence is longer than the universe has been running.
    fn next_seq(&mut self) -> u64 {
        self.next_write += 1;
        self.next_write
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
    pub(crate) fn is_dirty(&self, id: EntryId, cx: &App) -> bool {
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
                let stamp = loaded.stamp;
                let buffer = cx.new(|_| Buffer::new(loaded.text));
                this.track(id, buffer.clone(), path, stamp, cx);
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
    pub(crate) fn adopt(
        &mut self,
        buffer: Entity<Buffer>,
        path: PathBuf,
        stamp: Stamp,
        cx: &mut Context<Self>,
    ) {
        self.track(EntryId::DETACHED, buffer, path, stamp, cx);
        self.current = Some(EntryId::DETACHED);
    }

    /// Binds a buffer to a file and starts watching it. The observer is the
    /// whole autosave trigger: it fires on every edit and asks for a write.
    ///
    /// It must not lease the buffer it observes — a second lease of one entity
    /// panics (`gpui/src/app/entity_map.rs:231`) — which is why it only schedules.
    fn track(
        &mut self,
        id: EntryId,
        buffer: Entity<Buffer>,
        path: PathBuf,
        stamp: Stamp,
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
                writer: Arc::new(Writer {
                    state: futures::lock::Mutex::new(WriterState {
                        // Zero is below every minted sequence, which starts at
                        // one, so the first write of a note is never mistaken
                        // for a stale one.
                        written: 0,
                        stamp,
                    }),
                }),
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

    /// Writes every dirty note, once, as the application quits.
    ///
    /// Split deliberately. The synchronous half runs while the store is still
    /// alive: `App::shutdown` invokes the quit observers (`gpui/src/app.rs:1060`)
    /// *before* `windows.clear()` (`:1064`), so the buffers, the vault and the
    /// paths are all still reachable here. Everything the write needs is copied
    /// out now.
    ///
    /// The returned future touches no `App` at all, and must not: shutdown
    /// holds the `AppCell` borrowed across the whole poll loop
    /// (`gpui/src/app.rs:1004-1013`), and `AsyncApp::update` would panic on the
    /// second borrow. It also does no bookkeeping — nothing reads
    /// `saved_version` after the process exits.
    ///
    /// Each write is spawned onto the **background** executor. A foreground
    /// task cannot advance during the block (`test_scheduler.rs:362`), so a
    /// flush that awaited one would wait out the timeout and write nothing. The
    /// `Task`s are moved into the future rather than detached, because a
    /// dropped `Task` is cancelled.
    ///
    /// Does not preempt or cancel the pending chain: it cannot: see `Writer`
    /// for why the two writers are ordered instead of serialised.
    fn flush_all(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        // Collected before any sequence is minted, so the `self.open` borrow
        // ends before `next_seq` needs `&mut self`.
        let dirty: Vec<_> = self
            .open
            .iter()
            .filter_map(|(id, note)| {
                let buffer = note.buffer.read(cx);
                if buffer.version() == note.saved_version {
                    return None;
                }
                // Resolved exactly as `write` does, and for the same reason: a
                // rename moves the entry and never refreshes `OpenNote.path`.
                let path = if *id == EntryId::DETACHED {
                    note.path.clone()
                } else {
                    self.vault.as_ref()?.read(cx).absolute(*id)?
                };
                Some((Arc::clone(&note.writer), path, buffer.text().to_owned()))
            })
            .collect();

        let fs = Arc::clone(&self.fs);
        let writes: Vec<_> = dirty
            .into_iter()
            .map(|(writer, path, text)| {
                // Always the highest sequence that note will ever see, which is
                // what makes the flush the winner rather than merely a
                // contender. Every sequence is minted under `&mut App`; a chain
                // write that already minted one did so before this call, and
                // one that has not cannot mint during the block, because the
                // foreground session is where it would have to run.
                let seq = self.next_seq();
                cx.background_spawn(write_through(writer, Arc::clone(&fs), path, text, seq))
            })
            .collect();

        async move {
            futures::future::join_all(writes).await;
        }
    }

    /// Lists the vault again, so a file this process did not create through
    /// the editor reaches the tree.
    ///
    /// The only caller is the conflict path, and deliberately so: a full
    /// directory walk every `AUTOSAVE_DELAY` would not be free, and a conflict
    /// is rare. It is also the first production caller of the scan at all —
    /// nothing rescans after startup, so without this the preserved file would
    /// sit on disk unmentioned until the next launch, and the only signal this
    /// feature has would not fire.
    ///
    /// Two halves because `Vault::apply` needs `&mut Vault` and the walk
    /// awaits: an entity hands out `&mut` only inside a synchronous closure.
    /// Takes `&self` rather than reading the entity back: the only caller runs
    /// inside an `update`, which already holds the lease, and a second read of
    /// one entity panics (`gpui/src/app/entity_map.rs:156`).
    fn rescan(&self, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let fs = Arc::clone(&self.fs);
        cx.spawn(async move |_, cx| {
            let root = vault.read_with(cx, |vault, _| vault.root().to_path_buf());
            // `cx.spawn` is the *foreground* executor
            // (`gpui/src/app.rs:2036-2045`), and `RealFs::read_dir` blocks
            // inside its future by design — every other `Fs` call in this file
            // is wrapped the same way. A directory walk on the main thread
            // freezes the window, and a vault on a network mount is exactly
            // where both conflicts and slow walks come from.
            let walk = cx.background_spawn(async move { Vault::collect(&*fs, &root).await });
            let Ok(children) = walk.await else {
                return;
            };
            // The entity is held strongly by this closure, so neither this nor
            // the `read_with` above can fail on a released entity.
            vault.update(cx, |vault, cx| {
                vault.apply(children);
                cx.notify();
            });
        })
        .detach();
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
        let Ok(Some((fs, writer, path, text, version, seq))) = this.update(cx, |this, cx| {
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
            let writer = Arc::clone(&note.writer);
            let fs = Arc::clone(&this.fs);
            let seq = this.next_seq();
            Some((fs, writer, path, text, version, seq))
        }) else {
            return;
        };

        let written = cx
            .background_spawn(write_through(writer, fs, path, text, seq))
            .await;
        // Overtaken by a newer write, which is only reachable from the quit
        // flush. Not a failure: the file holds text newer than this one's, so
        // reporting it would be wrong and advancing `saved_version` would claim
        // this write's version reached disk when the other one's did.
        let Some(written) = written else {
            return;
        };
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
            if let Some(preserved) = saved.preserved.clone() {
                cx.emit(ConflictPreserved {
                    id,
                    path: preserved,
                });
                this.rescan(cx);
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

    use inkra_vault::{Mtime, Stamp};

    use super::{AUTOSAVE_DELAY, ConflictPreserved, Fs, NoteStore, SaveFailed};
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
        let store = cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)));
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
    /// Returns the store as well as the window: it is built here, so a test
    /// that needs to subscribe to it or read `is_dirty` takes it from the
    /// helper rather than reaching through `AppShell`, whose fields are private
    /// to `shell.rs`.
    fn open_shell(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (VisualTestContext, Entity<NoteStore>) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        // Loaded rather than a literal that happens to agree with the file:
        // `adopt` needs the real stamp, and a fabricated one would make the
        // first write of every test built on this helper see a mismatch, mint
        // a conflict file, and quietly defeat the tests that count the vault.
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
            move |_, cx| AppShell::new(editor, notes, cx)
        });
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();
        (VisualTestContext::from_window(window.into(), cx), notes)
    }

    /// Focus is in `EditorView`; the binding is scoped to `AppShell`. Proves
    /// the action dispatches up the tree rather than being swallowed.
    #[gpui::test]
    fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
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

        let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md")))
            .expect("the file was just inserted");
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new(loaded.text.clone())));
        let notes = cx.update(|cx| {
            cx.new(|cx| {
                let mut store = NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None, cx);
                store.adopt(
                    buffer.clone(),
                    PathBuf::from("/vault/a.md"),
                    loaded.stamp,
                    cx,
                );
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

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
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

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
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

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
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

    /// `⌘S` routed through the same queue is absorbed by the pending-chain
    /// guard unless it preempts: without the preemption it writes nothing until
    /// the periodic deadline arrives, which breaks the one promise the gesture
    /// makes. The clock never advances here, so only a
    /// preempting `⌘S` can produce the write.
    #[gpui::test]
    fn cmd_s_preempts_a_pending_autosave(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
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

    /// The defect the expectation's placement exists to avoid, in its simplest
    /// reachable form. The second write's expectation cannot be the stamp the
    /// *load* returned: our own first write already invalidated it. A note
    /// taking its expectation from `OpenNote` would mint a conflict file
    /// holding its own text on the second keystroke of an ordinary session.
    #[gpui::test]
    fn a_second_write_does_not_conflict_with_the_first(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();
        visual.simulate_input("Y");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeXY")
        );
        let entries =
            futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
        assert_eq!(
            entries.len(),
            1,
            "a write must not conflict with our own previous one"
        );
    }

    /// The same, with the second write *issued* while the first is in flight:
    /// `block_save` holds the periodic chain's write open past the turn that
    /// started it, so the `⌘S` below is decided against a file our own write has
    /// not finished changing.
    ///
    /// The two writes do not overlap at the filesystem, and cannot: the `⌘S`
    /// chain awaits the one it replaced (`schedule_save` takes `previous` rather
    /// than dropping it), so the second `write_through` starts after the first
    /// returned. Only the quit flush escapes that chaining — see
    /// `a_quit_flush_overtakes_the_write_it_overlaps`.
    #[gpui::test]
    fn an_overlapping_write_does_not_conflict_with_the_one_in_flight(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let gate = fs.block_save("/vault/a.md");
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        visual.simulate_input("Y");
        visual.simulate_keystrokes("cmd-s");
        gate.release();
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        let entries =
            futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
        assert_eq!(
            entries.len(),
            1,
            "neither write may conflict with the other"
        );
    }

    /// The two writers of one note, genuinely concurrent — the interleaving
    /// `Writer` exists for, and the one the test above cannot reach.
    ///
    /// Every write issued through `schedule_save` queues behind the previous
    /// one, so no two of them are ever in flight together. The quit flush is the
    /// single exception, and cannot be made to chain: `App::shutdown` blocks the
    /// foreground session while it polls the quit futures, so a flush that
    /// awaited the chain would wait on a task that can never resume. It issues
    /// its own write instead, and for that moment the note has two.
    ///
    /// Driven through `flush_all` rather than `cx.quit()`, because the gate has
    /// to be released from outside and `quit` does not return until the flush it
    /// is waiting for has finished. What quit adds on top of this — that the
    /// observer is registered at all, and that every dirty note is collected —
    /// is `quitting_writes_every_dirty_note`'s claim, not this one's.
    ///
    /// Both assertions bite, and on different defects. A flush that carried its
    /// own `Writer`, or the note's load stamp rather than what the chain's write
    /// had just produced, meets a file it does not recognise and preserves our
    /// own text into a conflict file. A flush whose sequence did not exceed the
    /// chain's is discarded as overtaken, and the file keeps text the user had
    /// already typed past.
    #[gpui::test]
    async fn a_quit_flush_overtakes_the_write_it_overlaps(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let gate = fs.block_save("/vault/a.md");

        // Parks the periodic chain inside `fs.save`, holding the note's lock:
        // `write_through` takes it before the write and keeps it across.
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        // Text the parked write cannot be carrying, so which write landed last
        // is readable from the file alone.
        visual.simulate_input("Y");
        // Held, never dropped: the flush's writes are `Task`s moved into this
        // future, and dropping a `Task` cancels it.
        let flush = notes.update(cx, NoteStore::flush_all);

        gate.release();
        flush.await;

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeXY"),
            "the flush carries the highest sequence, so it is the write that stands"
        );
        let entries =
            futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
        assert_eq!(
            entries.len(),
            1,
            "the flush expects what the chain's write produced, not what the load returned"
        );
    }

    /// An external writer changes the note between the load and the write. Its
    /// text survives under a new name, and the note holds what is on screen.
    #[gpui::test]
    fn an_external_change_is_preserved_beside_the_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let announced = Arc::new(AtomicUsize::new(0));
        let _guard = cx.update(|cx| {
            let announced = Arc::clone(&announced);
            cx.subscribe(&notes, move |_, _: &ConflictPreserved, _| {
                announced.fetch_add(1, Ordering::SeqCst);
            })
        });

        // `insert_file` keeps the path's identity, exactly as a second process
        // rewriting a file in place would.
        fs.insert_file("/vault/a.md", "theirs");

        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX"),
            "the note keeps its path and holds what is on screen"
        );
        let entries =
            futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
        let preserved = entries
            .iter()
            .find(|entry| entry.path.to_string_lossy().contains("conflict"))
            .expect("a conflict file");
        assert_eq!(
            fs.read_to_string(&preserved.path).as_deref(),
            Some("theirs")
        );
        assert_eq!(announced.load(Ordering::SeqCst), 1);
    }

    /// The conflict file's only route to the user. Nothing else in the running
    /// application rescans, so without this the file sits on disk unmentioned
    /// until the next launch.
    ///
    /// `open_two_notes`, not `open_shell`: the latter is the DETACHED path —
    /// `NoteStore::new(fs, None, cx)` — and has no vault at all, so the rescan
    /// branch is unreachable under it. This test needs no window; it asserts on
    /// the vault's contents, not on a frame.
    #[gpui::test]
    async fn a_conflict_reaches_the_vault_tree(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let buffer = store
            .update(cx, |store, cx| store.open_note(a, cx))
            .await
            .expect("the note opens");
        store.update(cx, |store, cx| store.set_current(a, cx));
        let before = store.read_with(cx, |store, cx| {
            store.vault().expect("a vault").read(cx).entries().count()
        });

        fs.insert_file("/vault/a.md", "theirs");
        // `update` plus an explicit `notify`: the observer `track` installs is
        // the autosave trigger, and `Buffer` cannot notify for itself. Without
        // the notify nothing schedules and the write never happens.
        buffer.update(cx, |buffer, cx| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset(0), "X");
            });
            cx.notify();
        });
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        let after = store.read_with(cx, |store, cx| {
            store.vault().expect("a vault").read(cx).entries().count()
        });
        assert_eq!(after, before + 1, "the preserved file is listed");
    }

    /// An ordinary write must not walk the directory. A conflict is rare;
    /// `AUTOSAVE_DELAY` is not.
    #[gpui::test]
    async fn an_ordinary_write_does_not_rescan(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let buffer = store
            .update(cx, |store, cx| store.open_note(a, cx))
            .await
            .expect("the note opens");
        store.update(cx, |store, cx| store.set_current(a, cx));

        // A scan of an unreadable directory yields nothing, so a rescan here
        // would empty the tree. Counting afterwards is what makes this test
        // about scanning rather than about the file's text.
        fs.make_unreadable("/vault");
        buffer.update(cx, |buffer, cx| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset(0), "X");
            });
            cx.notify();
        });
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("Xalpha")
        );
        let entries = store.read_with(cx, |store, cx| {
            store.vault().expect("a vault").read(cx).entries().count()
        });
        assert_eq!(entries, 2, "no rescan ran, so the tree is intact");
    }

    fn writer() -> Arc<super::Writer> {
        Arc::new(super::Writer {
            state: futures::lock::Mutex::new(super::WriterState {
                written: 0,
                // These tests drive `write_through` directly against a file
                // they wrote themselves, so the seed is never what the note
                // carries — which is the point: the first write of each is
                // expected to find a mismatch and preserve, and they assert on
                // the note's final text, which is unaffected either way.
                stamp: Stamp {
                    mtime: Mtime(std::time::SystemTime::UNIX_EPOCH),
                    size: 0,
                },
            }),
        })
    }

    /// The ordering guard, driven directly rather than raced.
    ///
    /// A `#[gpui::test]` cannot put this under test honestly. `TestScheduler`
    /// runs every runnable on the one thread driving the block
    /// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:308-417`),
    /// so it can model "queued and not yet polled" but never the production
    /// hazard, which is a background thread already inside `rename()`. And a
    /// test that let the scheduler pick the order would assert whatever seed 0
    /// happens to produce, for as long as gpui's scheduling weights stay put.
    ///
    /// Driving `write_through` by hand fixes both: it needs no window, no
    /// scheduler and no clock, so each interleaving is chosen rather than
    /// hoped for. The `scroll.rs` precedent — a plain `#[test]` in `app` for
    /// something that cannot move down a layer.
    #[test]
    fn a_later_write_wins_when_it_lands_second() {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let writer = writer();
        let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

        let older = super::write_through(
            Arc::clone(&writer),
            Arc::clone(&dyn_fs),
            PathBuf::from("/vault/a.md"),
            "older".to_owned(),
            1,
        );
        let newer = super::write_through(
            writer,
            dyn_fs,
            PathBuf::from("/vault/a.md"),
            "newer".to_owned(),
            2,
        );
        assert!(futures::executor::block_on(older).is_some_and(|r| r.is_ok()));
        assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("newer")
        );
    }

    /// The R2 revert, forced. Without the sequence the older write lands last
    /// and the file holds text the user had already replaced — the loss this
    /// whole feature exists to prevent, arriving through the quit flush that is
    /// supposed to be the safety net.
    #[test]
    fn a_later_write_wins_when_it_lands_first() {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let writer = writer();
        let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

        let newer = super::write_through(
            Arc::clone(&writer),
            Arc::clone(&dyn_fs),
            PathBuf::from("/vault/a.md"),
            "newer".to_owned(),
            2,
        );
        let older = super::write_through(
            writer,
            dyn_fs,
            PathBuf::from("/vault/a.md"),
            "older".to_owned(),
            1,
        );
        assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

        // `None`, not `Some(Err(_))`: overtaken is not failed, and a caller
        // that reported it would raise `SaveFailed` for a write that was
        // simply too late to matter.
        assert!(
            futures::executor::block_on(older).is_none(),
            "the stale write must be skipped, not merely reported"
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("newer"),
            "the older write must not have reached the file at all"
        );
    }

    /// The defect the cancel channel introduced, reachable only with a write
    /// slow enough to outlive the turn that issued it.
    ///
    /// `⌘S` installs a chain whose timer is already ready, then the next
    /// keystroke replaces `note.cancel` and drops that chain's sender. The
    /// receiver resolves `Err(Canceled)`, and treating that as a cancel makes
    /// the gesture that promises "write now" write nothing until the periodic
    /// deadline. The clock never advances past the release here, so only a
    /// chain that distinguishes a dropped sender from a deliberate send can
    /// produce the assertion below.
    #[gpui::test]
    fn a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let gate = fs.block_save("/vault/a.md");

        // Parks the periodic chain inside its write, so everything below queues
        // behind a write that is still in flight.
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        visual.simulate_keystrokes("cmd-s");
        // Lands while the `⌘S` chain is queued: it replaces the sender that
        // chain is selecting on.
        visual.simulate_input("Y");

        gate.release();
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeXY"),
            "the cmd-s chain must reach its write once the one it queued \
             behind finishes"
        );
    }

    /// `SaveFailed` is the only report a failed write has. Nothing else in the
    /// suite can fail one, so without this the emit — and `AppShell`'s
    /// subscription to it — can be deleted with the workspace still green.
    #[gpui::test]
    fn a_failed_write_reports_it_and_leaves_the_note_dirty(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        fs.make_unwritable("/vault/a.md");

        let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let failures = Arc::new(AtomicUsize::new(0));
        let _guard = cx.update(|cx| {
            let failures = Arc::clone(&failures);
            cx.subscribe(&notes, move |_, _: &SaveFailed, _| {
                failures.fetch_add(1, Ordering::SeqCst);
            })
        });

        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(failures.load(Ordering::SeqCst), 1, "the write failed once");
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("before")
        );
        notes.update(cx, |notes, cx| {
            assert!(
                notes.is_dirty(EntryId::DETACHED, cx),
                "a note whose write failed stays dirty, so the next edit retries"
            );
        });
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

        // A rescan is what create/rename/delete will trigger on every mutation
        // once they exist — deferred, with its trigger in the vault spec.
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

    /// The gap `⌘Q` used to leave: every dirty note reaches disk, not just the
    /// one `⌘S` would have picked.
    ///
    /// Quit through `TestAppContext::quit`, which calls `App::shutdown`
    /// directly (`gpui/src/app/test_context.rs:233`). `cx.quit()` inside an
    /// `update` would be `App::quit` — `self.platform.quit()`, whose test
    /// implementation is an empty body — and would prove nothing.
    ///
    /// Claims only that the flush is wired up and completes. It does **not**
    /// claim the 200 ms `SHUTDOWN_TIMEOUT` is met: under `TestScheduler` the
    /// block's bound is a random tick count and the clock jumps to the next
    /// timer without consulting any deadline
    /// (`scheduler/src/test_scheduler.rs:110-118`, `:441-448`).
    #[gpui::test]
    fn quitting_writes_every_dirty_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        store.update(cx, |store, cx| store.open_note(b, cx).detach());
        cx.run_until_parked();
        // Only `b` is current, so a flush that reused `save_current` would
        // write `b` and lose `a`.
        store.update(cx, |store, cx| store.set_current(b, cx));
        for id in [a, b] {
            store.update(cx, |store, cx| {
                let buffer = store.buffer_for(id).cloned().expect("the note is open");
                buffer.update(cx, |buffer, _| {
                    buffer.transact_at(Instant::now(), |buffer| {
                        buffer.insert(ByteOffset(0), "EDITED ");
                    });
                });
            });
        }

        cx.quit();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("EDITED alpha"),
            "the note that was not current must still be written"
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/b.md")).as_deref(),
            Some("EDITED beta")
        );
    }

    /// The version comparison, at quit. Without it every note on screen is
    /// rewritten on every exit, which on a synced vault is a burst of
    /// no-op changes the sync client still has to carry.
    #[gpui::test]
    fn quitting_leaves_a_clean_note_alone(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        // Text the buffer never held, so any write at all shows.
        fs.insert_file("/vault/a.md", "on disk, not in the buffer");
        fs.insert_file("/vault/b.md", "beta");
        let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        store.update(cx, |store, cx| store.open_note(a, cx).detach());
        cx.run_until_parked();

        cx.quit();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("on disk, not in the buffer")
        );
    }

    /// The file holds text the buffer never had, so a write of any kind shows —
    /// putting the version comparison under test, not just a matching write.
    #[gpui::test]
    fn cmd_s_on_an_untouched_note_leaves_the_file_alone(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "on disk, not in the buffer");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_keystrokes("cmd-s");
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("on disk, not in the buffer")
        );
    }
}
