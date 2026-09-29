//! Everything the store keeps per `EntryId`, in one map, so a note's parts cannot disagree.

mod lifecycle;
mod queue;
mod writer;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::channel::oneshot;
use gpui::{App, Entity, Subscription};
use inkra_text::{Buffer, Diff};
use inkra_vault::{Claims, EntryId, Mtime, Stamp};

pub(super) use lifecycle::{Lifecycle, Location, PendingSave, Swap};
use queue::Queue;
pub(super) use writer::{Folder, Writer, write_through};

use crate::buffer_events::Reloaded;

pub(super) struct OpenNote {
    buffer: Entity<Buffer>,
    /// Paired with the `Writer`'s stamp: `landed` and `did_reload` are its only writers.
    saved_version: u64,
    /// Shared with the quit flush too — a second `Writer` reintroduces the write race.
    writer: Arc<Writer>,
    /// Dropped with the note: a closed note schedules no save.
    _observer: Subscription,
}

impl OpenNote {
    /// `stamp`: the file the buffer was loaded from.
    pub(super) fn new(buffer: Entity<Buffer>, stamp: Stamp, observer: Subscription) -> Self {
        Self {
            buffer,
            saved_version: 0,
            writer: Arc::new(Writer::new(stamp)),
            _observer: observer,
        }
    }

    pub(super) fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
    }

    /// Closes the open transaction; the text and its version, unless that version is saved.
    pub(super) fn unsaved(&self, cx: &mut App) -> Option<(Arc<str>, u64)> {
        // `update`, never `as_mut`: the latter notifies on drop even
        // though `finalize_transaction` changes no text.
        let (text, version) = self.buffer.update(cx, |buffer, _| {
            buffer.finalize_transaction();
            (Arc::<str>::from(buffer.text()), buffer.version())
        });
        (version != self.saved_version).then_some((text, version))
    }

    /// A write of `version` landed; `write_through` moved the stamp under the `Writer`'s lock.
    pub(super) fn landed(&mut self, version: u64) {
        self.saved_version = version;
    }

    pub(super) fn is_dirty(&self, cx: &App) -> bool {
        self.buffer.read(cx).version() != self.saved_version
    }

    /// The text and its version, read together: a diff of the text applies only at that version.
    pub(super) fn snapshot(&self, cx: &App) -> (Arc<str>, u64) {
        let buffer = self.buffer.read(cx);
        (Arc::from(buffer.text()), buffer.version())
    }

    /// `None` when the buffer moved since the snapshot: it is dirty and keeps its edits.
    pub(super) fn did_reload(
        &mut self,
        diff: &Arc<Diff>,
        stamp: Stamp,
        cx: &mut App,
    ) -> Option<Adopt> {
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
        Some(Adopt(self.writer(), stamp))
    }

    pub(super) fn writer(&self) -> Arc<Writer> {
        Arc::clone(&self.writer)
    }

    #[cfg(test)]
    pub(super) fn saved_version(&self) -> u64 {
        self.saved_version
    }
}

/// A reload's stamp, adopted by the `Writer` when awaited: after the apply, or a refused reload's
/// next save would drop their text unpreserved (watcher spec, F8).
#[must_use]
pub(super) struct Adopt(Arc<Writer>, Stamp);

impl Adopt {
    pub(super) async fn run(self) {
        self.0.adopt(self.1).await;
    }
}

#[derive(Default)]
pub(super) struct NoteSession {
    queue: Queue,
    pub(super) life: Lifecycle<OpenNote>,
}

impl NoteSession {
    /// Holds nothing a later operation needs: a closed note schedules no save, and its chain finished.
    fn is_dormant(&self) -> bool {
        self.life.is_closed() && self.queue.is_idle()
    }
}

#[derive(Default)]
pub(super) struct Sessions {
    all: HashMap<EntryId, NoteSession>,
    /// The ids whose lifecycle holds an `OpenNote`: a swap runs on every save, so it walks these alone.
    live: HashSet<EntryId>,
}

impl Sessions {
    pub(super) fn get(&self, id: EntryId) -> Option<&NoteSession> {
        self.all.get(&id)
    }

    pub(super) fn note(&self, id: EntryId) -> Option<&OpenNote> {
        self.all.get(&id)?.life.bound()
    }

    pub(super) fn note_mut(&mut self, id: EntryId) -> Option<&mut OpenNote> {
        self.all.get_mut(&id)?.life.bound_mut()
    }

    fn queue_mut(&mut self, id: EntryId) -> &mut Queue {
        &mut self.all.entry(id).or_default().queue
    }

    pub(super) fn arm(&mut self, id: EntryId, delay: Duration) -> Option<oneshot::Receiver<()>> {
        self.queue_mut(id).arm(delay)
    }

    pub(super) fn timer_passed(&mut self, id: EntryId) {
        if let Some(session) = self.all.get_mut(&id) {
            session.queue.timer_passed();
        }
    }

    /// A link finished: the last one idles the chain, and a dormant session goes.
    fn settle(&mut self, id: EntryId, link: u64) {
        let Some(session) = self.all.get_mut(&id) else {
            return;
        };
        session.queue.finish(link);
        if session.is_dormant() {
            self.all.remove(&id);
        }
    }

    #[cfg(test)]
    pub(super) fn ids(&self) -> HashSet<EntryId> {
        self.all.keys().copied().collect()
    }

    #[cfg(test)]
    pub(super) fn live_ids(&self) -> HashSet<EntryId> {
        self.live.clone()
    }

    #[cfg(test)]
    pub(super) fn is_scheduled(&self, id: EntryId) -> bool {
        self.all
            .get(&id)
            .is_some_and(|session| session.queue.is_scheduled())
    }

    /// The one change to whether a lifecycle binds a note, so `live` stays the ids it binds and a
    /// dormant session goes; `swapped` moves only a bound note's `Location`.
    pub(super) fn update<R>(
        &mut self,
        id: EntryId,
        change: impl FnOnce(&mut Lifecycle<OpenNote>) -> R,
    ) -> R {
        let session = self.all.entry(id).or_default();
        let result = change(&mut session.life);
        if session.life.bound().is_some() {
            self.live.insert(id);
        } else {
            self.live.remove(&id);
        }
        if session.is_dormant() {
            self.all.remove(&id);
        }
        result
    }

    pub(super) fn live(&self) -> impl Iterator<Item = (EntryId, &NoteSession)> {
        // Indexed: every live id has a session, or `update` is wrong.
        self.live.iter().map(|id| (*id, &self.all[id]))
    }

    /// Each bound note against a swap: the live entry's and the replaced state's mtimes, and the
    /// replaced state's path. Only the notes it changed.
    pub(super) fn swapped(
        &mut self,
        mtimes: impl Fn(EntryId) -> (Option<Mtime>, Option<Mtime>),
        last: impl Fn(EntryId) -> Option<PathBuf>,
    ) -> Vec<(EntryId, Swap)> {
        let Self { all, live } = self;
        live.iter()
            .filter_map(|&id| {
                let life = &mut all.get_mut(&id)?.life;
                let (now, before) = mtimes(id);
                let swap = life.swapped(now, before, || last(id));
                (swap != Swap::NONE).then_some((id, swap))
            })
            .collect()
    }

    /// Every bound id holds its entry against a reclaim; a closed or trashed session holds none.
    pub(super) fn claims(&self) -> Claims {
        Claims {
            open: self.live.clone(),
            claimants: self
                .live()
                .filter(|(_, session)| session.life.is_open())
                .map(|(id, session)| (id, session.life.gone_path().cloned()))
                .collect(),
        }
    }
}
