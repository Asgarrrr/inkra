//! Keeps a `NoteIndex` current: fed by the vault's scans and by our own writes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use gpui::{AppContext as _, AsyncApp, Context, Entity, EventEmitter, Subscription, WeakEntity};
use inkra_index::{NoteIndex, NoteSet, Source};
use inkra_vault::{EntryId, Fs, Kind, Mtime, Vault};

use crate::vault::{NoteSaved, NoteStore};

/// A build applied: note names and link resolution may have changed.
pub(crate) struct NotesChanged;

impl EventEmitter<NotesChanged> for IndexSync {}

pub(crate) struct IndexSync {
    fs: Arc<dyn Fs>,
    vault: Option<Entity<Vault>>,
    index: NoteIndex,
    /// Bumped on each change to `index`, so a reader can skip a notify that changed nothing it reads.
    revision: u64,
    /// The mtime each note was indexed at, or is queued to be.
    indexed: HashMap<EntryId, Mtime>,
    /// One counter orders loads and writes, as `NoteStore::next_write` orders writes.
    applied: HashMap<EntryId, u64>,
    next_seq: u64,
    queue: VecDeque<(EntryId, u64)>,
    /// The latest sequence queued per note; an older queue entry for it is skipped.
    queued: HashMap<EntryId, u64>,
    /// One note at a time, so a large vault does not open every file at once.
    working: bool,
    /// Builds started; a build applies only if it is still the latest.
    generation: u64,
    /// The generation `index` holds.
    built: u64,
    /// The note whose load is in flight: popped from `queue`, not applied yet.
    loading: Option<EntryId>,
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
        let mut sync = Self {
            fs,
            vault,
            index: NoteIndex::default(),
            revision: 0,
            indexed: HashMap::new(),
            applied: HashMap::new(),
            next_seq: 0,
            queue: VecDeque::new(),
            queued: HashMap::new(),
            working: false,
            generation: 0,
            built: 0,
            loading: None,
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
        // A note re-queued while its load is in flight is one note pending, not two.
        let loading = self.loading.filter(|id| !self.queued.contains_key(id));
        let pending = self.queued.len() + usize::from(loading.is_some());
        (total.saturating_sub(pending), total)
    }

    /// True before the first build applies or while loads are pending; a rebuild alone is not.
    pub(crate) fn is_building(&self) -> bool {
        let (loaded, total) = self.progress();
        (self.generation != 0 && self.built == 0) || loaded < total
    }

    fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// Builds the notes off the foreground; `apply_notes` swaps them in.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        let entries: Vec<(EntryId, String, Kind)> = vault
            .read(cx)
            .entries()
            .map(|entry| {
                let path = entry.path.as_path().to_string_lossy().into_owned();
                (entry.id, path, entry.kind)
            })
            .collect();
        self.generation += 1;
        let generation = self.generation;
        let build = cx.background_spawn(async move { NoteSet::build(entries) });
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| {
            let notes = build.await;
            let _ = this.update(cx, |this, cx| this.apply_notes(generation, notes, cx));
        })
        .detach();
    }

    /// Applies a build unless a newer one started, then queues every note whose mtime differs
    /// from the one it was indexed at.
    fn apply_notes(&mut self, generation: u64, notes: NoteSet, cx: &mut Context<Self>) {
        if generation != self.generation {
            // As costly to free as the set a build replaces.
            cx.background_spawn(async move { drop(notes) }).detach();
            return;
        }
        self.built = generation;
        let previous = self.index.set_notes(notes);
        self.revision += 1;
        // Freeing a 50k table takes about 9 ms, over the foreground budget.
        cx.background_spawn(async move { drop(previous) }).detach();
        cx.emit(NotesChanged);
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let vault = vault.read(cx);
        let notes: Vec<(EntryId, Mtime)> = vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        let live: HashSet<EntryId> = notes.iter().map(|(id, _)| *id).collect();
        self.indexed.retain(|id, _| live.contains(id));
        self.applied.retain(|id, _| live.contains(id));
        self.queued.retain(|id, _| live.contains(id));
        // A gone note is not pending; the worker still clears `loading` when its load returns.
        if self.loading.is_some_and(|id| !live.contains(&id)) {
            self.loading = None;
        }
        for (id, mtime) in notes {
            if self.indexed.get(&id) != Some(&mtime) {
                self.indexed.insert(id, mtime);
                let seq = self.next_seq();
                self.queue.push_back((id, seq));
                self.queued.insert(id, seq);
            }
        }
        if !self.working && !self.queue.is_empty() {
            self.working = true;
            // Detached: it holds only a weak handle, so it ends when this entity does.
            cx.spawn(async move |this, cx| Self::work(this, cx).await)
                .detach();
        }
        cx.notify();
    }

    async fn work(this: WeakEntity<Self>, cx: &mut AsyncApp) {
        loop {
            // `working` clears in the same update that finds the queue empty, so a
            // reconcile never queues behind a worker that is about to stop.
            let Ok(Some((id, seq, path, fs))) = this.update(cx, |this, cx| {
                let next = this.next_load(cx);
                if next.is_none() {
                    this.working = false;
                    cx.notify();
                }
                next
            }) else {
                return;
            };
            let loaded = cx
                .background_spawn(async move {
                    fs.load(&path).await.map(|loaded| {
                        let source = Source::parse(&loaded.text);
                        (source, Arc::<str>::from(loaded.text))
                    })
                })
                .await;
            let applied = this.update(cx, |this, cx| {
                this.loading = None;
                match loaded {
                    Ok((source, text)) => this.apply(id, seq, source, text, cx),
                    // Retried by the next reconcile; a GUI error has nowhere to go yet.
                    Err(_) => this.forget(id, seq, cx),
                }
            });
            if applied.is_err() {
                return;
            }
        }
    }

    fn next_load(
        &mut self,
        cx: &Context<Self>,
    ) -> Option<(EntryId, u64, std::path::PathBuf, Arc<dyn Fs>)> {
        let vault = self.vault.as_ref()?.read(cx);
        while let Some((id, seq)) = self.queue.pop_front() {
            if self.queued.get(&id) != Some(&seq) {
                continue;
            }
            self.queued.remove(&id);
            if let Some(path) = vault.absolute(id) {
                self.loading = Some(id);
                return Some((id, seq, path, Arc::clone(&self.fs)));
            }
        }
        None
    }

    /// Parsed off the main thread and never queued: a write needs no load, and must not wait
    /// behind a startup queue.
    fn saved(&mut self, _: Entity<NoteStore>, event: &NoteSaved, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let (id, text) = (event.id, Arc::clone(&event.text));
        let seq = self.next_seq();
        self.indexed.insert(id, event.stamp.mtime);
        let parsing = Arc::clone(&text);
        let parsed = cx.background_spawn(async move { Source::parse(&parsing) });
        cx.spawn(async move |this, cx| {
            let source = parsed.await;
            let _ = this.update(cx, |this, cx| this.apply(id, seq, source, text, cx));
        })
        .detach();
    }

    /// Drops links read before a failed load, unless a newer write already replaced them.
    fn forget(&mut self, id: EntryId, seq: u64, cx: &mut Context<Self>) {
        self.indexed.remove(&id);
        if self.applied.get(&id).is_some_and(|&last| seq < last) {
            return;
        }
        self.applied.insert(id, seq);
        self.index.remove(id);
        self.revision += 1;
        cx.notify();
    }

    fn apply(
        &mut self,
        id: EntryId,
        seq: u64,
        source: Source,
        text: Arc<str>,
        cx: &mut Context<Self>,
    ) {
        if self.applied.get(&id).is_some_and(|&last| seq < last) {
            return;
        }
        self.applied.insert(id, seq);
        self.index.insert(id, source, text);
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
