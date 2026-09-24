//! Keeps a `LinkIndex` current: fed by the vault's scans and by our own writes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use gpui::{AppContext as _, AsyncApp, Context, Entity, Subscription, WeakEntity};
use inkra_index::{LinkIndex, Source};
use inkra_vault::{EntryId, Fs, Kind, Mtime, Vault};

use crate::vault::{NoteSaved, NoteStore};

pub(crate) struct LinkSync {
    fs: Arc<dyn Fs>,
    vault: Option<Entity<Vault>>,
    index: LinkIndex,
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
    _subscriptions: Vec<Subscription>,
}

impl LinkSync {
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
            index: LinkIndex::default(),
            indexed: HashMap::new(),
            applied: HashMap::new(),
            next_seq: 0,
            queue: VecDeque::new(),
            queued: HashMap::new(),
            working: false,
            _subscriptions: subscriptions,
        };
        sync.reconcile(cx);
        sync
    }

    pub(crate) fn index(&self) -> &LinkIndex {
        &self.index
    }

    /// True while notes found by a scan are loading; a write's own parse does not count.
    pub(crate) fn is_building(&self) -> bool {
        self.working
    }

    fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// Queues every note whose mtime differs from the one it was indexed at.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let vault = vault.read(cx);
        self.index.set_notes(vault.entries());
        let notes: Vec<(EntryId, Mtime)> = vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        let live: HashSet<EntryId> = notes.iter().map(|(id, _)| *id).collect();
        self.indexed.retain(|id, _| live.contains(id));
        self.applied.retain(|id, _| live.contains(id));
        self.queued.retain(|id, _| live.contains(id));
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
                    fs.load(&path)
                        .await
                        .map(|loaded| Source::parse(&loaded.text))
                })
                .await;
            let applied = this.update(cx, |this, cx| match loaded {
                Ok(source) => this.apply(id, seq, source, cx),
                // Retried by the next reconcile; a GUI error has nowhere to go yet.
                Err(_) => this.forget(id, seq, cx),
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
        let parsed = cx.background_spawn(async move { Source::parse(&text) });
        cx.spawn(async move |this, cx| {
            let source = parsed.await;
            let _ = this.update(cx, |this, cx| this.apply(id, seq, source, cx));
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
        cx.notify();
    }

    fn apply(&mut self, id: EntryId, seq: u64, source: Source, cx: &mut Context<Self>) {
        if self.applied.get(&id).is_some_and(|&last| seq < last) {
            return;
        }
        self.applied.insert(id, seq);
        self.index.insert(id, source);
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
