use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use gpui::{AppContext as _, Context};
use inkra_index::NoteSet;
use inkra_vault::{Entry, EntryId, Mtime};

use super::diff::{Change, notes};
use super::{IndexSync, NotesChanged};
use crate::timeline::{Step, Timeline};

/// A build of the vault's notes, with the inputs of `queue_changed`, derived off the foreground.
pub(super) struct Built {
    pub(super) notes: NoteSet,
    /// Every note's id and mtime, in vault order.
    mtimes: Vec<(EntryId, Mtime)>,
    live: HashSet<EntryId>,
}

impl Built {
    pub(super) fn new(entries: &[Entry]) -> Self {
        let named = entries
            .iter()
            .map(|entry| {
                let path = entry.path.as_path().to_string_lossy().into_owned();
                (entry.id, path, entry.kind, entry.mtime)
            })
            .collect();
        let mtimes: Vec<(EntryId, Mtime)> = notes(entries)
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

/// One build at a time: a notify during it marks it `Dirty` rather than starting another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Build {
    #[default]
    Idle,
    Running,
    /// Running, and the vault notified since it read the entries: `reconcile` runs again once it applies.
    Dirty,
}

impl IndexSync {
    /// Diffs the vault against `last` off the foreground; `apply_change` applies the result.
    pub(super) fn reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        // One build, after the full scan: the root pass alone would cost a build the next swap discards.
        if !vault.read(cx).is_scanned() {
            return;
        }
        // Dropping the build in flight instead would starve the index under a stream of notifies.
        if self.build != Build::Idle {
            self.build = Build::Dirty;
            return;
        }
        let started = Instant::now();
        let entries = vault.read(cx).shared_entries();
        self.build = Build::Running;
        #[cfg(test)]
        {
            self.reconciles += 1;
        }
        let read_seq = self.next_seq;
        let last = self.last.clone();
        #[cfg(test)]
        let hold = self.hold.clone();
        let read = Arc::clone(&entries);
        let building = cx.background_spawn(async move {
            #[cfg(test)]
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            Change::between(last.as_deref(), &read)
        });
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| {
            let change = building.await;
            let dirty = Self::timed_turn(&this, cx, |this, cx| {
                this.apply_change(read_seq, entries, change, cx)
            }) == Some(true);
            // Outside the turn: `reconcile` may mark `Indexed`, which must wait for the settle the turn starts.
            if dirty {
                let _ = this.update(cx, Self::reconcile);
            }
        })
        .detach();
        Timeline::turn(started.elapsed(), cx);
        // Before the first build applies `is_building` holds, so this marks nothing yet.
        self.mark_indexed(cx);
    }

    /// Applies a change read from `entries` and queues each changed note; true when the vault
    /// notified meanwhile. `seq` is the counter at the read.
    fn apply_change(
        &mut self,
        seq: u64,
        entries: Arc<[Entry]>,
        change: Change,
        cx: &mut Context<Self>,
    ) -> bool {
        let dirty = std::mem::take(&mut self.build) == Build::Dirty;
        // Likely its last reference once the vault swapped; freeing 50k entries here could cost milliseconds.
        let previous = self.last.replace(entries);
        cx.background_spawn(async move { drop(previous) }).detach();
        match change {
            Change::Layout(built) => self.apply_notes(seq, *built, cx),
            Change::Mtimes(moved) => self.queue_changed(seq, moved),
        }
        self.retry_forgotten(seq, cx);
        self.load_more(cx);
        cx.notify();
        dirty
    }

    /// Swaps in a build's notes, dropping what it no longer lists.
    fn apply_notes(&mut self, seq: u64, built: Built, cx: &mut Context<Self>) {
        let Built {
            notes,
            mtimes,
            live,
        } = built;
        self.note_sets += 1;
        let previous = self.index.set_notes(notes);
        self.revision += 1;
        Timeline::mark(Step::NamesReady, cx);
        // Freeing a 50k table takes about 9 ms, over the foreground budget.
        cx.background_spawn(async move { drop(previous) }).detach();
        cx.emit(NotesChanged);
        self.indexed.retain(|id, _| live.contains(id));
        self.latest.retain(|id, _| live.contains(id));
        self.queued.retain(|id, _| live.contains(id));
        self.forgotten.retain(|id| live.contains(id));
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
        self.queue_changed(seq, mtimes);
        cx.background_spawn(async move { drop(live) }).detach();
    }

    /// Queues each forgotten note still in the vault; after a `Layout`, `queue_changed` finds it queued already.
    fn retry_forgotten(&mut self, seq: u64, cx: &Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        // The live vault, not `last`: it finds an id in O(1), and the snapshot has no index by id.
        let vault = vault.read(cx);
        let retry = self
            .forgotten
            .iter()
            .filter_map(|&id| vault.entry(id))
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        self.queue_changed(seq, retry);
    }

    /// Queues each note whose mtime is not the one indexed.
    fn queue_changed(&mut self, seq: u64, mtimes: Vec<(EntryId, Mtime)>) {
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
    }
}
