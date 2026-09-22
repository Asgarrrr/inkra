use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::channel::oneshot;
use gpui::{App, Context, Entity, Subscription, Task, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Stamp, Vault};

use super::ops::Ops;
use super::writer::{Writer, write_through};
use super::{ConflictPreserved, SaveFailed};

/// How long a note may stay dirty before it is written. Obsidian's cadence: the
/// write lands this long after typing *starts*, not this long after it stops.
/// An idle debounce — Zed's `AutosaveSetting::AfterDelay` — bounds nothing,
/// because a long uninterrupted writing session never reaches idle, and that is
/// the session whose loss hurts most.
///
/// A constant rather than a setting: this repo has no settings system, and
/// adding one to hold a single number is the wrong order.
pub(super) const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

pub(super) struct OpenNote {
    buffer: Entity<Buffer>,
    /// Where the note was when it was opened. Only `EntryId::DETACHED` reads
    /// it; every other note resolves its path against the vault at write time,
    /// because a rename moves it and nothing refreshes this.
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
    /// Shared with every write of this note, and with the quit flush. See
    /// `Writer`.
    writer: Arc<Writer>,
}

pub(crate) struct NoteStore {
    pub(super) fs: Arc<dyn Fs>,
    /// `None` when launched on a single file, or on nothing.
    pub(super) vault: Option<Entity<Vault>>,
    pub(super) open: HashMap<EntryId, OpenNote>,
    /// One per note that has ever had an operation queued on it, kept
    /// afterwards. Not pruned when a note closes: the chain is what orders a
    /// rename against a load, and a closed note can still be renamed.
    ///
    /// Grows monotonically, including for notes since deleted — an entry holds
    /// no buffer and no text, so a session's worth is nothing, but it is not
    /// bounded by the vault's size. Pruning waits for the same trigger as
    /// buffer eviction.
    pub(super) ops: HashMap<EntryId, Ops>,
    pub(super) current: Option<EntryId>,
    /// One per open note, for as long as it is open. A buffer with no observer
    /// never schedules a write, so a note edited and then switched away from
    /// would keep its changes only in memory.
    pub(super) observers: HashMap<EntryId, Subscription>,
    /// Stamps every write with its issue order. Minted where the text is
    /// captured, under `&mut App`, so the order is the order the writes were
    /// decided — which `Buffer::version` cannot give: an undo *lowers* the
    /// version (`Buffer::version`), so a flush after an undo would
    /// carry a lower number than the write it has to beat.
    next_write: u64,
    /// The pending vault walk. One for the whole store, not one per note: a
    /// walk rebuilds the entire listing, so two of them collide whatever caused
    /// them. See `rescan`.
    pub(super) rescan: Option<Task<()>>,
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
            ops: HashMap::new(),
            current: None,
            observers: HashMap::new(),
            next_write: 0,
            rescan: None,
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
                writer: Arc::new(Writer::new(stamp)),
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
    pub(super) fn schedule_save(&mut self, id: EntryId, delay: Duration, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        let ops = self.ops.entry(id).or_default();
        if delay.is_zero() {
            // Errs when the chain is already past its timer, which is exactly
            // the case where there is nothing left to preempt.
            if let Some(cancel) = ops.cancel.take() {
                let _ = cancel.send(());
            }
        } else if ops.scheduled {
            return;
        }

        let (cancel, cancelled) = oneshot::channel();
        ops.cancel = Some(cancel);
        ops.scheduled = !delay.is_zero();
        // Taken, not dropped: the new link awaits it below. Replacing the slot
        // outright would cancel an operation that has already been queued.
        let previous = ops.chain.take();

        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
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
                if let Some(ops) = this.ops.get_mut(&id) {
                    ops.scheduled = false;
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
    pub(super) fn flush_all(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
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
                // A note the user has sent to the trash is not written on the
                // way out. This flush cannot join the note's chain, so it mints
                // the highest sequence that note will ever see — which would
                // drop the queued trash as stale and leave a confirmed delete
                // silently undone. Writing it would also recreate the file.
                if self.ops.get(id).is_some_and(|ops| ops.trashing) {
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
                // The entry being gone means something outside this process
                // moved or removed the file. Reported rather than swallowed,
                // which is what this used to do — every autosave from then on
                // did nothing at all, forever, with no signal. Rescans now
                // happen on every mutation, so an entry disappearing
                // mid-session is reachable far sooner than it was.
                let resolved = this.vault.as_ref().and_then(|v| v.read(cx).absolute(id));
                let Some(path) = resolved else {
                    cx.emit(SaveFailed(id));
                    return None;
                };
                path
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
                drop(this.rescan(cx));
            }
            cx.notify();
        });
    }
}
