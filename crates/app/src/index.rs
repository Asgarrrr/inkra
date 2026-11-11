//! Keeps a `NoteIndex` current: fed by the vault's scans and by our own writes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, Priority, Subscription, WeakEntity, actions,
};
use inkra_index::live::{Effects, Status, Turn};
use inkra_index::{FileRead, LiveIndex, NoteContent, NoteIndex, TEXT_CACHE_BYTES, TextCache};
use inkra_vault::{EntryId, Fs, Vault};

use crate::timeline::{Step, Timeline};
use crate::vault::{NoteSaved, NoteStore};

mod cache_file;
mod cache_writer;
mod loading;
mod reads;
mod scrub;
mod watch;

use cache_writer::CacheWriter;
use loading::Landed;
pub(crate) use reads::{READERS, Reader};
#[cfg(test)]
pub(crate) use watch::BUILDING_REFRESH;
pub(crate) use watch::IndexWatch;

actions!(index, [RebuildIndex]);

/// A build applied: note names and link resolution may have changed.
pub(crate) struct NotesChanged;

impl EventEmitter<NotesChanged> for IndexSync {}

#[cfg(test)]
type Hold = futures::future::Shared<futures::channel::oneshot::Receiver<()>>;

pub(crate) struct IndexSync {
    fs: Arc<dyn Fs>,
    vault: Entity<Vault>,
    /// Pins the open notes' texts at each store change and search start.
    notes: WeakEntity<NoteStore>,
    live: LiveIndex,
    /// Filled by our saves and by the searches' reads.
    texts: Arc<Mutex<TextCache>>,
    /// Writes the cache file `new` reads; `None` keeps no cache (P17).
    writer: Option<CacheWriter>,
    /// `simulate_load`'s loads, added to the index's revision.
    #[cfg(test)]
    simulated_loads: u64,
    /// Where a finished load goes; `drain` applies it on the foreground.
    landed: UnboundedSender<Landed>,
    /// Replaces `APPLY_BUDGET`; zero makes each turn apply one load.
    #[cfg(test)]
    apply_budget: Option<std::time::Duration>,
    #[cfg(test)]
    turns: usize,
    /// A build waits for it before it starts, so a test can hold one in flight.
    #[cfg(test)]
    hold: Option<Hold>,
    _subscriptions: [Subscription; 3],
}

impl IndexSync {
    /// `cache`: the index cache file, read beside the scan; `None` keeps no cache (P17).
    pub(crate) fn new(
        notes: &Entity<NoteStore>,
        vault: Entity<Vault>,
        cache: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let fs = notes.read(cx).fs();
        let subscriptions = [
            cx.subscribe(notes, Self::saved),
            cx.observe(&vault, |this, _, cx| this.reconcile(cx)),
            cx.observe(notes, |this, notes, cx| {
                reads::lock(&this.texts).pin_only(&notes.read(cx).open_notes());
            }),
        ];
        let (landed, arrivals) = unbounded();
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| Self::drain(this, arrivals, cx).await)
            .detach();
        let mut sync = Self {
            fs,
            vault,
            notes: notes.downgrade(),
            live: LiveIndex::new(cx.background_executor().num_cpus() * loading::LOADS_PER_CORE),
            texts: Arc::new(Mutex::new(TextCache::new(TEXT_CACHE_BYTES))),
            writer: None,
            #[cfg(test)]
            simulated_loads: 0,
            landed,
            #[cfg(test)]
            apply_budget: None,
            #[cfg(test)]
            turns: 0,
            #[cfg(test)]
            hold: None,
            _subscriptions: subscriptions,
        };
        if let Some(file) = cache {
            // Before the first turn: no note queues until the outcome lands (P7).
            sync.live.await_cache();
            let root = sync.vault.read(cx).root().to_path_buf();
            sync.writer = Some(CacheWriter::new(
                Arc::clone(&sync.fs),
                file.clone(),
                root.clone(),
            ));
            let reading = cx.background_spawn(cache_file::read(Arc::clone(&sync.fs), file, root));
            // Detached, holding a weak handle: an outcome landing after this entity is gone is dropped.
            cx.spawn(async move |this, cx| {
                let outcome = reading.await;
                let _ = this.update(cx, |this, cx| {
                    Timeline::mark(Step::Cached(outcome.name()), cx);
                    this.turn(cx, |turn| turn.cached(outcome));
                });
            })
            .detach();
        }
        sync.reconcile(cx);
        sync
    }

    // Readers only delegate to `live` or read `vault`, so D5's `Seen` check covers them.
    pub(crate) fn revision(&self) -> u64 {
        #[cfg(not(test))]
        let simulated = 0;
        #[cfg(test)]
        let simulated = self.simulated_loads;
        self.live.revision() + simulated
    }

    /// A load applying: the index changed, and every reader is notified.
    #[cfg(test)]
    pub(crate) fn simulate_load(&mut self, cx: &mut Context<Self>) {
        self.simulated_loads += 1;
        cx.notify();
    }

    pub(crate) fn index(&self) -> &NoteIndex {
        self.live.index()
    }

    pub(crate) fn status(&self) -> Status {
        self.live.status()
    }

    /// Runs one timed turn and its effects.
    fn turn<R>(&mut self, cx: &mut Context<Self>, inputs: impl FnOnce(&mut Turn<'_>) -> R) -> R {
        let started = Instant::now();
        let (result, effects) = self.live.turn(self.vault.read(cx), inputs);
        let held = effects.indexed.as_ref().map(|indexed| indexed.held);
        let seeded = effects.seeded;
        self.run(effects, cx);
        let took = started.elapsed();
        Timeline::turn(took, cx);
        if seeded {
            Timeline::seed_turn(took, cx);
        }
        if let Some(held) = held {
            self.mark_indexed(cx);
            if let Some(writer) = &mut self.writer {
                writer.start(held);
            }
        }
        self.write_if_due(cx);
        result
    }

    /// Runs a turn's effects; the `Indexed` mark stays with `turn`, outside the timing.
    fn run(&mut self, effects: Effects, cx: &mut Context<Self>) {
        let Effects {
            loads,
            build,
            merge,
            retired,
            layout,
            notify,
            indexed,
            seeded: _,
        } = effects;
        for load in loads {
            self.spawn_load(load, cx);
        }
        if let Some(job) = indexed.and_then(|indexed| indexed.scrub) {
            self.scrub(job, cx);
        }
        if let Some(job) = build {
            #[cfg(test)]
            let hold = self.hold.clone();
            let building = cx.background_spawn(async move {
                #[cfg(test)]
                if let Some(hold) = hold {
                    let _ = hold.await;
                }
                job.run()
            });
            // Detached, holding a weak handle: a result landing after this entity is gone is dropped.
            cx.spawn(async move |this, cx| {
                let built = building.await;
                let _ = this.update(cx, |this, cx| this.turn(cx, |turn| turn.built(built)));
            })
            .detach();
        }
        if let Some(merge) = merge {
            // Low: a query's scan goes first.
            let running = cx
                .background_executor()
                .spawn_with_priority(Priority::Low, async move { merge.run() });
            // Detached, holding a weak handle: a result landing after this entity is gone is dropped.
            cx.spawn(async move |this, cx| {
                let merged = running.await;
                let _ = this.update(cx, |this, cx| this.turn(cx, |turn| turn.merged(merged)));
            })
            .detach();
        }
        if let Some(retired) = retired {
            // Freeing a 50k table takes about 9 ms, over the foreground budget.
            cx.background_spawn(async move { drop(retired) }).detach();
        }
        if layout {
            Timeline::mark(Step::NamesReady, cx);
            cx.emit(NotesChanged);
        }
        if notify {
            cx.notify();
        }
    }

    /// The startup timeline's mark, once nothing is left to load and the settle has applied.
    fn mark_indexed(&mut self, cx: &mut Context<Self>) {
        let index = self.live.index();
        let corpus = index.corpus();
        let step = Step::Indexed {
            notes: corpus.len(),
            corpus_bytes: corpus.heap_bytes(),
            words_bytes: index.words().heap_bytes(),
            names_bytes: index.names().heap_bytes(),
        };
        Timeline::mark(step, cx);
    }

    /// The vault changed: a turn starts the build once the vault is scanned.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        // Typed, or clippy asks for the path `Turn::vault_changed`, which binds one lifetime and fails the bound.
        self.turn(cx, |turn: &mut Turn<'_>| turn.vault_changed());
    }

    /// Removes the cache file, then loads every note again (P15).
    pub(crate) fn rebuild(&mut self, cx: &mut Context<Self>) {
        if let Some(writer) = &mut self.writer {
            writer.remove(cx);
        }
        self.turn(cx, |turn: &mut Turn<'_>| turn.rebuild());
    }

    /// Notes a search read changed or gone since their load: each loads again.
    pub(crate) fn stale(&mut self, ids: &[EntryId], cx: &mut Context<Self>) {
        self.turn(cx, |turn| turn.stale(ids));
    }

    /// Never queued: a write needs no load and must not wait behind the startup queue.
    fn saved(&mut self, _: Entity<NoteStore>, event: &NoteSaved, cx: &mut Context<Self>) {
        let (id, stamp) = (event.id, event.stamp);
        reads::lock(&self.texts).insert(id, Arc::clone(&event.text), stamp);
        let ticket = self.turn(cx, |turn| turn.saved(id, stamp));
        let text = Arc::clone(&event.text);
        let file = FileRead {
            stamp,
            identity: event.identity,
        };
        let parsed = cx.background_spawn(async move { NoteContent::parse(&text, file) });
        cx.spawn(async move |this, cx| {
            let content = parsed.await;
            let _ = this.update(cx, |this, cx| {
                this.turn(cx, |turn| turn.landed(ticket, Ok(content)));
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod corpus_tests;

#[cfg(test)]
mod startup_tests;

#[cfg(test)]
mod loading_tests;

#[cfg(test)]
mod reads_tests;

#[cfg(test)]
mod scanning_tests;

#[cfg(test)]
mod words_tests;

#[cfg(test)]
mod cache_file_tests;

#[cfg(test)]
mod cache_writer_tests;

#[cfg(test)]
mod rebuild_tests;

#[cfg(test)]
mod scrub_tests;
