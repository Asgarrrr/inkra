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
use super::scans::Scans;
use super::writer::{Writer, write_through};
use super::{ConflictPreserved, NoteSaved, SaveFailed};

/// Periodic, not an idle debounce — a debounce never fires during one long session.
pub(crate) const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

pub(super) struct OpenNote {
    buffer: Entity<Buffer>,
    /// A rename never updates this; resolve against the vault at write time instead.
    path: PathBuf,
    saved_version: u64,
    /// Shared with the quit flush too — a second `Writer` reintroduces the write race.
    writer: Arc<Writer>,
}

pub(crate) struct NoteStore {
    pub(super) fs: Arc<dyn Fs>,
    pub(super) vault: Option<Entity<Vault>>,
    pub(super) open: HashMap<EntryId, OpenNote>,
    /// Not pruned on close: still orders a rename issued after the note closes.
    pub(super) ops: HashMap<EntryId, Ops>,
    pub(super) current: Option<EntryId>,
    pub(super) observers: HashMap<EntryId, Subscription>,
    /// `Buffer::version` can't order writes: undo lowers it.
    next_write: u64,
    pub(super) scans: Scans,
    /// Dropping this cancels the quit flush.
    _quit: Subscription,
}

impl NoteStore {
    pub(crate) fn new(
        fs: Arc<dyn Fs>,
        vault: Option<Entity<Vault>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut store = Self {
            fs,
            vault,
            open: HashMap::new(),
            ops: HashMap::new(),
            current: None,
            observers: HashMap::new(),
            next_write: 0,
            scans: Scans::default(),
            _quit: cx.on_app_quit(Self::flush_all),
        };
        if store
            .vault
            .as_ref()
            .is_some_and(|vault| !vault.read(cx).is_scanned())
        {
            store.start_scan(Vec::new(), cx);
        }
        store
    }

    fn next_seq(&mut self) -> u64 {
        self.next_write += 1;
        self.next_write
    }

    pub(crate) fn fs(&self) -> Arc<dyn Fs> {
        Arc::clone(&self.fs)
    }

    pub(crate) fn vault(&self) -> Option<&Entity<Vault>> {
        self.vault.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn buffer_for(&self, id: EntryId) -> Option<&Entity<Buffer>> {
        self.open.get(&id).map(|note| &note.buffer)
    }

    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<EntryId> {
        self.current
    }

    pub(crate) fn is_dirty(&self, id: EntryId, cx: &App) -> bool {
        self.open
            .get(&id)
            .is_some_and(|note| note.buffer.read(cx).version() != note.saved_version)
    }

    /// Does not set `current`: it resolves on the load's schedule, not screen order.
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
                // Two clicks on one unopened note race; the second must not
                // replace a buffer already typed into.
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

    /// Schedules a save of the note being left before switching, so `⌘S`
    /// keeps writing to the right target.
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

    /// Must not lease the buffer it observes — a second lease of one entity panics.
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
                saved_version: 0,
                writer: Arc::new(Writer::new(stamp)),
            },
        );
    }

    pub(super) fn schedule_save(&mut self, id: EntryId, delay: Duration, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        let ops = self.ops.entry(id).or_default();
        if delay.is_zero() {
            if let Some(cancel) = ops.cancel.take() {
                let _ = cancel.send(());
            }
        } else if ops.scheduled {
            return;
        }

        let (cancel, cancelled) = oneshot::channel();
        ops.cancel = Some(cancel);
        ops.scheduled = !delay.is_zero();
        // Taken, not dropped: replacing the slot outright would cancel an
        // operation already queued behind it.
        let previous = ops.chain.take();

        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
            let timer = cx.background_executor().timer(delay).fuse();
            if let Some(previous) = previous {
                previous.await;
            }
            futures::select_biased! {
                // A dropped sender also lands here; treating it as cancel
                // too is why `⌘S` then a keystroke can write nothing.
                cancelled = cancelled.fuse() => if cancelled.is_ok() { return },
                () = timer.fuse() => {}
            }
            // Before the write, not after, so a mid-write edit queues the
            // next write instead of being lost.
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

    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current {
            self.schedule_save(id, Duration::ZERO, cx);
        }
    }

    /// Copies buffers out synchronously: shutdown holds the `AppCell`
    /// borrowed, so a later borrow here would panic.
    pub(super) fn flush_all(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        let dirty: Vec<_> = self
            .open
            .iter()
            .filter_map(|(id, note)| {
                let buffer = note.buffer.read(cx);
                if buffer.version() == note.saved_version {
                    return None;
                }
                // Skips a note mid-trash: this flush can't join its chain,
                // so writing here would recreate the file being deleted.
                if self.ops.get(id).is_some_and(|ops| ops.trashing) {
                    return None;
                }
                let path = if *id == EntryId::DETACHED {
                    note.path.clone()
                } else {
                    self.vault.as_ref()?.read(cx).absolute(*id)?
                };
                Some((Arc::clone(&note.writer), path, Arc::from(buffer.text())))
            })
            .collect();

        let fs = Arc::clone(&self.fs);
        let writes: Vec<_> = dirty
            .into_iter()
            .map(|(writer, path, text)| {
                let seq = self.next_seq();
                cx.background_spawn(write_through(writer, Arc::clone(&fs), path, text, seq))
            })
            .collect();

        async move {
            futures::future::join_all(writes).await;
        }
    }

    async fn write(id: EntryId, this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp) {
        let Ok(Some((fs, writer, path, text, version, seq))) = this.update(cx, |this, cx| {
            let note = this.open.get(&id)?;
            // `update`, never `as_mut`: the latter notifies on drop even
            // though `finalize_transaction` changes no text.
            let (text, version) = note.buffer.update(cx, |buffer, _| {
                buffer.finalize_transaction();
                (Arc::<str>::from(buffer.text()), buffer.version())
            });
            if version == note.saved_version {
                return None;
            }
            let path = if id == EntryId::DETACHED {
                note.path.clone()
            } else {
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
            .background_spawn(write_through(writer, fs, path, Arc::clone(&text), seq))
            .await;
        // `None` means a newer write already landed; not a failure.
        let Some(written) = written else {
            return;
        };
        let _ = this.update(cx, |this, cx| {
            let Ok(saved) = written else {
                cx.emit(SaveFailed(id));
                return;
            };
            if let Some(note) = this.open.get_mut(&id) {
                note.saved_version = version;
            }
            // An atomic save moves the inode: without the re-key the id orphans on the next rescan.
            this.reidentify(id, saved.identity, saved.stamp.mtime, cx);
            // The index's only signal for an ordinary write: it neither rescans nor notifies.
            if id != EntryId::DETACHED {
                cx.emit(NoteSaved {
                    id,
                    text,
                    stamp: saved.stamp,
                });
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
