//! Keeps a `NoteIndex` current: fed by the vault's scans and by our own writes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{AppContext as _, AsyncApp, Context, Entity, EventEmitter, Subscription, WeakEntity};
use inkra_index::{NoteContent, NoteIndex, NoteSet};
use inkra_vault::{Entry, EntryId, Fs, Kind, Mtime, Vault};

use crate::timeline::{Step, Timeline, timed};
use crate::vault::{NoteSaved, NoteStore};

mod loading;
mod merging;
mod watch;

use loading::{Landed, Loading};
pub(crate) use watch::{BUILDING_REFRESH, IndexWatch, Refresh};

/// A build applied: note names and link resolution may have changed.
pub(crate) struct NotesChanged;

impl EventEmitter<NotesChanged> for IndexSync {}

/// A build of the vault's notes, with the inputs of `apply_notes`'s diff, derived off the foreground.
struct Built {
    notes: NoteSet,
    /// Every note's id and mtime, in vault order.
    mtimes: Vec<(EntryId, Mtime)>,
    live: HashSet<EntryId>,
}

impl Built {
    fn new(entries: &[Entry]) -> Self {
        let named = entries
            .iter()
            .map(|entry| {
                let path = entry.path.as_path().to_string_lossy().into_owned();
                (entry.id, path, entry.kind, entry.mtime)
            })
            .collect();
        let mtimes: Vec<(EntryId, Mtime)> = entries
            .iter()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        let live = mtimes.iter().map(|(id, _)| *id).collect();
        Self {
            notes: NoteSet::build(named),
            mtimes,
            live,
        }
    }
}

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
    /// Builds started; a build applies only if it is still the latest.
    generation: u64,
    /// The generation `index` holds; `None` before the first build applies.
    built: Option<u64>,
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
            generation: 0,
            built: None,
            loading: Vec::new(),
            landed,
            #[cfg(test)]
            apply_budget: None,
            #[cfg(test)]
            turns: 0,
            #[cfg(test)]
            merges: 0,
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
        self.vault.is_some() && self.built.is_none()
    }

    fn next_seq(&mut self, id: EntryId) -> u64 {
        self.next_seq += 1;
        self.latest.insert(id, self.next_seq);
        self.next_seq
    }

    fn is_latest(&self, id: EntryId, seq: u64) -> bool {
        self.latest.get(&id) == Some(&seq)
    }

    /// Builds the notes off the foreground; `apply_notes` swaps them in.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        // One build, after the full scan: the root pass alone would cost a build the next swap discards.
        if !vault.read(cx).is_scanned() {
            return;
        }
        let started = Instant::now();
        let entries = vault.read(cx).shared_entries();
        self.generation += 1;
        let (generation, seq) = (self.generation, self.next_seq);
        let building = cx.background_spawn(async move { Built::new(&entries) });
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| {
            let built = building.await;
            Self::timed_turn(&this, cx, |this, cx| {
                this.apply_notes(generation, seq, built, cx);
            });
        })
        .detach();
        Timeline::turn(started.elapsed(), cx);
        // Before the first build applies `is_building` holds, so this marks nothing yet.
        self.mark_indexed(cx);
    }

    /// Applies a build unless a newer one started, then queues each changed note; `seq` is the counter at its read.
    fn apply_notes(&mut self, generation: u64, seq: u64, built: Built, cx: &mut Context<Self>) {
        if generation != self.generation {
            // As costly to free as the set a build replaces.
            cx.background_spawn(async move { drop(built) }).detach();
            return;
        }
        let Built {
            notes,
            mtimes,
            live,
        } = built;
        self.built = Some(generation);
        let previous = self.index.set_notes(notes);
        self.revision += 1;
        Timeline::mark(Step::NamesReady, cx);
        // Freeing a 50k table takes about 9 ms, over the foreground budget.
        cx.background_spawn(async move { drop(previous) }).detach();
        cx.emit(NotesChanged);
        self.indexed.retain(|id, _| live.contains(id));
        self.latest.retain(|id, _| live.contains(id));
        self.queued.retain(|id, _| live.contains(id));
        // A load in flight for a note gone is not pending; it keeps its slot until it lands.
        for load in &mut self.loading {
            load.live &= live.contains(&load.id);
        }
        // Reserved for the notes not yet indexed, so no insert rehashes (open-first spec, O7).
        let missing = mtimes.len().saturating_sub(self.indexed.len());
        self.indexed.reserve(missing);
        self.latest.reserve(missing);
        self.queued.reserve(missing);
        self.queue.reserve(missing);
        for (id, mtime) in mtimes {
            // A save since the build read the vault holds a newer mtime than the build's.
            let current = self.indexed.get(&id).is_some_and(|&indexed| {
                indexed == mtime || self.latest.get(&id).is_some_and(|&latest| latest > seq)
            });
            if !current {
                self.indexed.insert(id, mtime);
                self.queue_load(id);
            }
        }
        cx.background_spawn(async move { drop(live) }).detach();
        self.load_more(cx);
        cx.notify();
    }

    fn queue_load(&mut self, id: EntryId) {
        let seq = self.next_seq(id);
        self.queue.push_back((id, seq));
        self.queued.insert(id, seq);
    }

    /// One timed foreground update and the word merge it made due, then `mark_indexed`; `None`
    /// once this entity is gone.
    fn timed_turn(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        update: impl FnOnce(&mut Self, &mut Context<Self>),
    ) -> Option<()> {
        timed(cx, |cx| {
            this.update(cx, |this, cx| {
                let building = this.is_building();
                update(this, cx);
                // On the edge where the startup load ends, whichever turn ends it (a load or a rebuild).
                this.settling |= building && !this.marked_indexed && !this.is_building();
                this.merge_words(cx);
            })
        })
        .ok()?;
        this.update(cx, Self::mark_indexed).ok()
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
        self.index.remove(id);
        self.revision += 1;
        cx.notify();
    }

    fn apply(&mut self, id: EntryId, seq: u64, content: NoteContent, cx: &mut Context<Self>) {
        if !self.is_latest(id, seq) {
            return;
        }
        self.index.insert(id, content);
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
