use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{App, Context, Entity, Subscription, Task, prelude::*};
use inkra_text::{Buffer, Diff};
use inkra_vault::{EntryId, Fs, Pending, Scans, Stamp, Vault};

use super::ops::Ops;
use super::writer::{Folder, Writer};
use crate::buffer_events::Reloaded;

/// Periodic, not an idle debounce — a debounce never fires during one long session.
pub(crate) const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

pub(super) struct OpenNote {
    pub(super) buffer: Entity<Buffer>,
    pub(super) location: Location,
    pub(super) saved_version: u64,
    /// Shared with the quit flush too — a second `Writer` reintroduces the write race.
    pub(super) writer: Arc<Writer>,
}

/// Where the note's file is; only a note outside the vault's entries carries a path.
pub(super) enum Location {
    /// Resolved against the vault at write time: a rename never updates the note.
    Vault,
    /// `EntryId::DETACHED`, a file opened outside any vault.
    Detached(PathBuf),
    /// The file left the vault; the path is its last vault path (watcher spec, F9).
    Gone(PathBuf),
}

impl OpenNote {
    pub(super) fn is_gone(&self) -> bool {
        matches!(self.location, Location::Gone(_))
    }

    pub(super) fn folder(&self) -> Folder {
        if self.is_gone() {
            Folder::Create
        } else {
            Folder::MustExist
        }
    }

    /// The text and its version, read together: a diff of the text applies only at that version.
    pub(super) fn snapshot(&self, cx: &App) -> (Arc<str>, u64) {
        let buffer = self.buffer.read(cx);
        (Arc::from(buffer.text()), buffer.version())
    }

    /// `None` when the buffer moved since the snapshot: it is dirty and keeps its edits.
    pub(super) fn did_reload(&mut self, diff: &Arc<Diff>, cx: &mut App) -> Option<Arc<Writer>> {
        let applied = self.buffer.update(cx, |buffer, cx| {
            let applied = buffer.apply_diff(Instant::now(), diff);
            if applied && !diff.hunks().is_empty() {
                cx.emit(Reloaded(Arc::clone(diff)));
                cx.notify();
            }
            applied
        });
        if !applied {
            return None;
        }
        self.saved_version = self.buffer.read(cx).version();
        Some(self.writer())
    }

    pub(super) fn writer(&self) -> Arc<Writer> {
        Arc::clone(&self.writer)
    }

    #[cfg(test)]
    pub(super) fn saved_version(&self) -> u64 {
        self.saved_version
    }
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
    _watch: Option<Task<()>>,
    /// Parks the next reload between its diff and its apply (watcher spec, test 11b).
    #[cfg(test)]
    pub(super) held_reload: Option<futures::channel::oneshot::Receiver<()>>,
    /// Dropping this cancels the quit flush.
    _quit: Subscription,
}

impl NoteStore {
    pub(crate) fn new(
        fs: Arc<dyn Fs>,
        vault: Option<Entity<Vault>>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Before the initial scan, so no change falls between its reads and the watch (watcher spec, F2).
        let root = vault
            .as_ref()
            .map(|vault| vault.read(cx).root().to_path_buf());
        let watch = root.and_then(|root| Self::watch(&*fs, &root, cx));
        let mut store = Self {
            fs,
            vault,
            open: HashMap::new(),
            ops: HashMap::new(),
            current: None,
            observers: HashMap::new(),
            next_write: 0,
            scans: Scans::default(),
            _watch: watch,
            #[cfg(test)]
            held_reload: None,
            _quit: cx.on_app_quit(Self::flush_all),
        };
        if store
            .vault
            .as_ref()
            .is_some_and(|vault| !vault.read(cx).is_scanned())
        {
            store.rescan_changed(Pending::Everything, cx);
        }
        store
    }

    pub(super) fn next_seq(&mut self) -> u64 {
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

    pub(crate) fn is_gone(&self, id: EntryId) -> bool {
        self.open.get(&id).is_some_and(OpenNote::is_gone)
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
            let loaded = cx
                .background_spawn(async move { fs.load(&path).await })
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
                this.track(id, buffer.clone(), Location::Vault, stamp, cx);
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
        self.track(
            EntryId::DETACHED,
            buffer,
            Location::Detached(path),
            stamp,
            cx,
        );
        self.current = Some(EntryId::DETACHED);
    }

    /// Must not lease the buffer it observes — a second lease of one entity panics.
    fn track(
        &mut self,
        id: EntryId,
        buffer: Entity<Buffer>,
        location: Location,
        stamp: Stamp,
        cx: &mut Context<Self>,
    ) {
        let observer = cx.observe(&buffer, move |this, _buffer, cx| {
            // A clean notify (a reload) would hold the note's chain for the delay and write nothing.
            if this.is_dirty(id, cx) {
                this.schedule_save(id, AUTOSAVE_DELAY, cx);
            }
        });
        self.observers.insert(id, observer);
        self.open.insert(
            id,
            OpenNote {
                buffer,
                location,
                saved_version: 0,
                writer: Arc::new(Writer::new(stamp)),
            },
        );
    }
}
