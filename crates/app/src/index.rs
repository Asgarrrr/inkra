//! Keeps a `NoteIndex` current: fed by the vault's scans and by our own writes.

use std::sync::Arc;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    AppContext as _, AsyncApp, Context, Entity, EventEmitter, Priority, SharedString, Subscription,
    WeakEntity,
};
use inkra_index::live::{Merged, Status, Ticket, TurnEnd};
use inkra_index::{LiveIndex, NoteContent, NoteIndex};
use inkra_vault::{Fs, Vault};

use crate::count::thousands;
use crate::timeline::{Step, Timeline, timed};
use crate::vault::{NoteSaved, NoteStore};

mod building;
mod loading;
mod watch;

use loading::Landed;
#[cfg(test)]
pub(crate) use watch::BUILDING_REFRESH;
pub(crate) use watch::IndexWatch;

/// A picker's empty text before the first build.
pub(crate) const SCANNING: &str = "Scanning the vault…";

/// A picker's footer, "Indexing 1,204 of 50,000 notes", while `Indexing`; else `None`.
pub(crate) fn indexing_footer(status: Status) -> Option<SharedString> {
    let Status::Indexing { loaded, total } = status else {
        return None;
    };
    Some(
        format!(
            "Indexing {} of {} notes",
            thousands(loaded),
            thousands(total)
        )
        .into(),
    )
}

/// A build applied: note names and link resolution may have changed.
pub(crate) struct NotesChanged;

impl EventEmitter<NotesChanged> for IndexSync {}

#[cfg(test)]
type Hold = futures::future::Shared<futures::channel::oneshot::Receiver<()>>;

pub(crate) struct IndexSync {
    fs: Arc<dyn Fs>,
    vault: Option<Entity<Vault>>,
    live: LiveIndex,
    /// Bumped on each change to the index, so a reader can skip a notify that changed nothing it reads.
    revision: u64,
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
            live: LiveIndex::default(),
            revision: 0,
            landed,
            #[cfg(test)]
            apply_budget: None,
            #[cfg(test)]
            turns: 0,
            #[cfg(test)]
            hold: None,
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
        self.live.index()
    }

    /// `Ready` without a vault: nothing will ever load.
    pub(crate) fn status(&self) -> Status {
        match self.vault {
            Some(_) => self.live.status(),
            None => Status::Ready,
        }
    }

    /// One timed foreground update and the word merge it made due, then the `Indexed` mark; the
    /// update's result, or `None` once this entity is gone.
    fn timed_turn<R>(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        update: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> Option<R> {
        let (result, indexed) = timed(cx, |cx| {
            this.update(cx, |this, cx| {
                let result = update(this, cx);
                (result, this.end_turn(cx))
            })
        })
        .ok()?;
        if indexed {
            this.update(cx, Self::mark_indexed).ok()?;
        }
        Some(result)
    }

    /// Starts the merge the turn made due; whether the startup load was reported indexed.
    fn end_turn(&mut self, cx: &mut Context<Self>) -> bool {
        let TurnEnd { merge, indexed } = self.live.end_turn();
        if let Some(merge) = merge {
            // Low: a query's scan goes first.
            let running = cx
                .background_executor()
                .spawn_with_priority(Priority::Low, async move { merge.run() });
            // Detached, holding a weak handle: a result landing after this entity is gone is dropped.
            cx.spawn(async move |this, cx| {
                let merged = running.await;
                Self::timed_turn(&this, cx, |this, cx| this.apply_merge(merged, cx));
            })
            .detach();
        }
        indexed
    }

    /// Notifies no reader, since a merge changes no answer.
    fn apply_merge(&mut self, merged: Merged, cx: &mut Context<Self>) {
        let retired = self.live.merged(merged);
        // Freeing a 50k-note segment would take milliseconds of the turn.
        cx.background_spawn(async move { drop(retired) }).detach();
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

    /// Never queued: a write needs no load and must not wait behind the startup queue.
    fn saved(&mut self, _: Entity<NoteStore>, event: &NoteSaved, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let ticket = self.live.saved(event.id, event.stamp.mtime);
        let text = Arc::clone(&event.text);
        let modified = event.stamp.mtime;
        let parsed = cx.background_spawn(async move { NoteContent::parse(text, modified) });
        cx.spawn(async move |this, cx| {
            let content = parsed.await;
            Self::timed_turn(&this, cx, |this, cx| this.land(ticket, Ok(content), cx));
        })
        .detach();
    }

    /// A load or a save's parse landed; readers are notified when the index changed.
    fn land(
        &mut self,
        ticket: Ticket,
        loaded: inkra_vault::Result<NoteContent>,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = &self.vault else {
            return;
        };
        if self.live.landed(ticket, loaded, vault.read(cx)) {
            self.revision += 1;
            cx.notify();
        }
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
