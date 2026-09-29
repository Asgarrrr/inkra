//! Builds: one at a time (L6), each applied, pruning what the vault no longer lists (L4) and
//! queuing each note whose mtime moved, unless a save holds a newer one (L2).

use std::collections::HashSet;
use std::mem;
use std::sync::Arc;

use inkra_vault::{Entry, EntryId, Mtime, Vault};

use super::LiveIndex;
use super::diff::{Change, notes};
use crate::note_set::NoteSet;

/// One build at a time: an input during it marks it `Dirty` rather than starting another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Build {
    #[default]
    Idle,
    Running,
    /// Running, and the vault changed since it read the entries: its apply asks for another.
    Dirty,
}

/// The diff of two entry snapshots; `run` it off the foreground.
pub struct BuildJob {
    /// The seq counter when the entries were read.
    seq: u64,
    last: Option<Arc<[Entry]>>,
    entries: Arc<[Entry]>,
}

/// A build's result, for `LiveIndex::apply_build`.
pub struct Built {
    seq: u64,
    entries: Arc<[Entry]>,
    change: Change,
}

/// A note set built off the foreground, with what `apply_build` queues and prunes by.
pub(super) struct Layout {
    pub(super) notes: NoteSet,
    /// Every note's id and mtime, in vault order.
    mtimes: Vec<(EntryId, Mtime)>,
    live: HashSet<EntryId>,
}

pub struct Applied {
    /// The vault changed while the build ran: call `start_build` again.
    pub dirty: bool,
    /// The note set changed: names and link resolution may differ.
    pub layout: bool,
    pub retired: Retired,
}

/// The previous note set, entries and live set; free them off the foreground.
#[must_use = "free it off the foreground"]
pub struct Retired {
    _last: Option<Arc<[Entry]>>,
    _notes: Option<(NoteSet, HashSet<EntryId>)>,
}

impl BuildJob {
    #[must_use]
    pub fn run(self) -> Built {
        Built {
            seq: self.seq,
            change: Change::between(self.last.as_deref(), &self.entries),
            entries: self.entries,
        }
    }
}

impl Layout {
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

impl LiveIndex {
    /// Vault entries changed. `None` while a build runs; that build's apply then returns `dirty`.
    #[must_use]
    pub fn start_build(&mut self, entries: Arc<[Entry]>) -> Option<BuildJob> {
        // Dropping the build in flight instead would starve the index under a stream of changes.
        if self.build != Build::Idle {
            self.build = Build::Dirty;
            return None;
        }
        self.build = Build::Running;
        Some(BuildJob {
            seq: self.next_seq,
            last: self.last.clone(),
            entries,
        })
    }

    /// Applies a build and queues each changed note; `vault` is the live one, to retry forgotten notes.
    pub fn apply_build(&mut self, built: Built, vault: &Vault) -> Applied {
        let Built {
            seq,
            entries,
            change,
        } = built;
        let dirty = mem::take(&mut self.build) == Build::Dirty;
        let last = self.last.replace(entries);
        let retired_notes = match change {
            Change::Layout(layout) => Some(self.apply_notes(seq, *layout)),
            Change::Mtimes(moved) => {
                self.queue_changed(seq, moved);
                None
            }
        };
        self.retry_forgotten(seq, vault);
        Applied {
            dirty,
            layout: retired_notes.is_some(),
            retired: Retired {
                _last: last,
                _notes: retired_notes,
            },
        }
    }

    /// Swaps in a build's notes, dropping what it no longer lists; the previous set and the live set.
    fn apply_notes(&mut self, seq: u64, layout: Layout) -> (NoteSet, HashSet<EntryId>) {
        let Layout {
            notes,
            mtimes,
            live,
        } = layout;
        self.note_sets += 1;
        let previous = self.index.set_notes(notes);
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
        (previous, live)
    }

    /// Queues each forgotten note still in the vault; after a `Layout`, `queue_changed` finds it queued already.
    fn retry_forgotten(&mut self, seq: u64, vault: &Vault) {
        // The live vault, not `last`: it finds an id in O(1), and the snapshot has no index by id.
        let retry = self
            .forgotten
            .iter()
            .filter_map(|&id| vault.entry(id))
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        self.queue_changed(seq, retry);
    }

    /// Queues each note whose mtime is not the one indexed; `seq` is the counter at the build's read.
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
