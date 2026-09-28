//! Keeps a `NoteIndex` current: fed by the vault's scans and by our own writes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{AppContext as _, AsyncApp, Context, Entity, EventEmitter, Subscription, WeakEntity};
use inkra_index::{NoteContent, NoteIndex};
use inkra_vault::{Entry, EntryId, Fs, Mtime, Vault};

use crate::timeline::{Step, Timeline, timed};
use crate::vault::{NoteSaved, NoteStore};

mod building;
mod diff;
mod loading;
mod merging;
mod watch;

use building::Build;
use loading::{Landed, Loading};
pub(crate) use watch::{BUILDING_REFRESH, IndexWatch, Refresh};

/// A build applied: note names and link resolution may have changed.
pub(crate) struct NotesChanged;

impl EventEmitter<NotesChanged> for IndexSync {}

#[cfg(test)]
type Hold = futures::future::Shared<futures::channel::oneshot::Receiver<()>>;

pub(crate) struct IndexSync {
    fs: Arc<dyn Fs>,
    vault: Option<Entity<Vault>>,
    index: NoteIndex,
    /// Bumped on each change to `index`, so a reader can skip a notify that changed nothing it reads.
    revision: u64,
    /// The mtime each note was indexed at, or is queued to be.
    indexed: HashMap<EntryId, Mtime>,
    /// One counter orders loads and writes, as `NoteStore::next_write` orders writes.
    next_seq: u64,
    /// The latest seq minted per note: only a load or save carrying it acts; an older one is superseded.
    latest: HashMap<EntryId, u64>,
    queue: VecDeque<(EntryId, u64)>,
    /// The latest sequence queued per note; an older queue entry for it is skipped.
    queued: HashMap<EntryId, u64>,
    /// Notes a failed load forgot; every apply queues them again, since an `Mtimes` change may not list them.
    forgotten: HashSet<EntryId>,
    build: Build,
    /// The vault entries the last applied change was read from; `None` until the first build applies.
    last: Option<Arc<[Entry]>>,
    /// Note sets applied; a failed load compares it to retry after a build that may have moved its note.
    note_sets: u64,
    /// At most one per core, each counted until the foreground applies it.
    loading: Vec<Loading>,
    /// Where a finished load goes; `drain` applies it on the foreground.
    landed: UnboundedSender<Landed>,
    /// Replaces `APPLY_BUDGET`; zero makes each turn apply one load.
    #[cfg(test)]
    apply_budget: Option<std::time::Duration>,
    #[cfg(test)]
    turns: usize,
    #[cfg(test)]
    merges: usize,
    #[cfg(test)]
    reconciles: usize,
    /// A build waits for it before it starts, so a test can hold one in flight.
    #[cfg(test)]
    hold: Option<Hold>,
    /// The startup load's last turn left nothing building; cleared once the settle merge it asks
    /// for applies, or finds nothing to merge.
    settling: bool,
    /// The `Indexed` mark is taken; every later turn skips it.
    marked_indexed: bool,
    _subscriptions: Vec<Subscription>,
}

impl IndexSync {
    pub(crate) fn new(notes: &Entity<NoteStore>, cx: &mut Context<Self>) -> Self {
        let fs = notes.read(cx).fs();
        let vault = notes.read(cx).vault().cloned();
        let mut subscriptions = vec![cx.subscribe(notes, Self::saved)];
        if let Some(vault) = &vault {
            subscriptions.push(cx.observe(vault, |this, _, cx| this.reconcile(cx)));
        }
        let (landed, arrivals) = unbounded();
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| Self::drain(this, arrivals, cx).await)
            .detach();
        let mut sync = Self {
            fs,
            vault,
            index: NoteIndex::default(),
            revision: 0,
            indexed: HashMap::new(),
            next_seq: 0,
            latest: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashMap::new(),
            forgotten: HashSet::new(),
            build: Build::Idle,
            last: None,
            note_sets: 0,
            loading: Vec::new(),
            landed,
            #[cfg(test)]
            apply_budget: None,
            #[cfg(test)]
            turns: 0,
            #[cfg(test)]
            merges: 0,
            #[cfg(test)]
            reconciles: 0,
            #[cfg(test)]
            hold: None,
            settling: false,
            marked_indexed: false,
            _subscriptions: subscriptions,
        };
        sync.reconcile(cx);
        sync
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// A load applying: the index changed, and every reader is notified.
    #[cfg(test)]
    pub(crate) fn simulate_load(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        cx.notify();
    }

    pub(crate) fn index(&self) -> &NoteIndex {
        &self.index
    }

    /// `(loaded, total)`: the notes of the last build, less those queued or loading.
    pub(crate) fn progress(&self) -> (usize, usize) {
        let total = self.index.names().len();
        // A note loading and queued again is one note pending.
        let loading = self
            .loading
            .iter()
            .filter(|load| load.live && !self.queued.contains_key(&load.id))
            .count();
        let pending = self.queued.len() + loading;
        (total.saturating_sub(pending), total)
    }

    /// True before the first build applies or while loads are pending; a rebuild alone is not.
    pub(crate) fn is_building(&self) -> bool {
        let (loaded, total) = self.progress();
        self.awaits_first_build() || loaded < total
    }

    /// Until the first build after the vault's full scan applies, there are no names to list.
    pub(crate) fn awaits_first_build(&self) -> bool {
        self.vault.is_some() && self.last.is_none()
    }

    fn next_seq(&mut self, id: EntryId) -> u64 {
        self.next_seq += 1;
        self.latest.insert(id, self.next_seq);
        self.next_seq
    }

    fn is_latest(&self, id: EntryId, seq: u64) -> bool {
        self.latest.get(&id) == Some(&seq)
    }

    fn queue_load(&mut self, id: EntryId) {
        let seq = self.next_seq(id);
        self.queue.push_back((id, seq));
        self.queued.insert(id, seq);
    }

    /// One timed foreground update and the word merge it made due, then `mark_indexed`; the
    /// update's result, or `None` once this entity is gone.
    fn timed_turn<R>(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        update: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> Option<R> {
        let result = timed(cx, |cx| {
            this.update(cx, |this, cx| {
                let building = this.is_building();
                let result = update(this, cx);
                // On the edge where the startup load ends, whichever turn ends it (a load or a rebuild).
                this.settling |= building && !this.marked_indexed && !this.is_building();
                this.merge_words(cx);
                result
            })
        })
        .ok()?;
        this.update(cx, Self::mark_indexed).ok()?;
        Some(result)
    }

    /// The first time nothing is left to load and the settle it asked for has applied, for the
    /// startup timeline.
    fn mark_indexed(&mut self, cx: &mut Context<Self>) {
        if self.marked_indexed || self.is_building() || self.settling {
            return;
        }
        self.marked_indexed = true;
        let corpus = self.index.corpus();
        let step = Step::Indexed {
            notes: corpus.len(),
            corpus_bytes: corpus.heap_bytes(),
            words_bytes: self.index.words().heap_bytes(),
            names_bytes: self.index.names().heap_bytes(),
        };
        Timeline::mark(step, cx);
    }

    /// Never queued: a write needs no load and must not wait behind the startup queue.
    fn saved(&mut self, _: Entity<NoteStore>, event: &NoteSaved, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let id = event.id;
        let seq = self.next_seq(id);
        self.indexed.insert(id, event.stamp.mtime);
        let text = Arc::clone(&event.text);
        let modified = event.stamp.mtime;
        let parsed = cx.background_spawn(async move { NoteContent::parse(text, modified) });
        cx.spawn(async move |this, cx| {
            let content = parsed.await;
            Self::timed_turn(&this, cx, |this, cx| this.apply(id, seq, content, cx));
        })
        .detach();
    }

    /// Drops links read before a failed load, unless a newer load or write is on its way.
    fn forget(&mut self, id: EntryId, seq: u64, cx: &mut Context<Self>) {
        if !self.is_latest(id, seq) {
            return;
        }
        self.indexed.remove(&id);
        self.forgotten.insert(id);
        self.index.remove(id);
        self.revision += 1;
        cx.notify();
    }

    fn apply(&mut self, id: EntryId, seq: u64, content: NoteContent, cx: &mut Context<Self>) {
        if !self.is_latest(id, seq) {
            return;
        }
        self.index.insert(id, content);
        self.forgotten.remove(&id);
        self.revision += 1;
        // Not a note of the index yet (its build is still running): the next diff reloads it.
        if !self.index.is_indexed(id) {
            self.indexed.remove(&id);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod corpus_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod startup_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod loading_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod scanning_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod words_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod diff_tests;
