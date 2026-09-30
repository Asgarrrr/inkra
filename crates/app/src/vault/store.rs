use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, Context, Entity, Subscription, Task, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Pending, Scans, Stamp, Vault};

use super::session::{Location, OpenNote, Sessions};

/// Periodic, not an idle debounce — a debounce never fires during one long session.
pub(crate) const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

pub(crate) struct NoteStore {
    pub(super) fs: Arc<dyn Fs>,
    pub(super) vault: Option<Entity<Vault>>,
    pub(super) sessions: Sessions,
    pub(super) current: Option<EntryId>,
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
            sessions: Sessions::default(),
            current: None,
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

    pub(crate) fn buffer_for(&self, id: EntryId) -> Option<&Entity<Buffer>> {
        self.sessions.note(id).map(OpenNote::buffer)
    }

    pub(crate) fn current(&self) -> Option<EntryId> {
        self.current
    }

    pub(crate) fn is_gone(&self, id: EntryId) -> bool {
        self.sessions
            .get(id)
            .is_some_and(|session| session.life.is_gone())
    }

    pub(crate) fn is_dirty(&self, id: EntryId, cx: &App) -> bool {
        self.sessions.note(id).is_some_and(|note| note.is_dirty(cx))
    }

    /// Does not set `current`: it resolves on the load's schedule, not screen order.
    pub(crate) fn open_note(
        &mut self,
        id: EntryId,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Buffer>>> {
        if let Some(note) = self.sessions.note(id) {
            return Task::ready(Some(note.buffer().clone()));
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
                if let Some(note) = this.sessions.note(id) {
                    return note.buffer().clone();
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
        if self.sessions.note(id).is_none() {
            return;
        }
        if let Some(leaving) = self.current.filter(|leaving| *leaving != id) {
            self.schedule_save(leaving, Duration::ZERO, cx);
        }
        self.current = Some(id);
        cx.notify();
    }

    /// An empty tab shows no note: the note left saves now, as `set_current` does.
    pub(crate) fn clear_current(&mut self, cx: &mut Context<Self>) {
        if let Some(left) = self.current.take() {
            self.schedule_save(left, Duration::ZERO, cx);
        }
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
        let note = OpenNote::new(buffer, stamp, observer);
        self.sessions.update(id, |life| life.open(location, note));
    }
}
