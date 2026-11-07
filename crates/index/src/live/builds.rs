//! Builds: one at a time (L6), each applied, pruning what the vault no longer lists (L4) and
//! queuing each note whose stamp moved, unless a save holds a newer one (L2).

use std::collections::HashSet;
use std::mem;
use std::sync::Arc;

use inkra_vault::{Entry, EntryId, Stamp, Vault};

use super::LiveIndex;
use super::diff::{Change, files, notes};
use crate::note_set::NoteSet;
use crate::persist::{Decoded, Outcome};
use crate::seed::Seed;
use crate::words::RetiredWords;

/// One build at a time: an input during it marks it `Dirty` rather than starting another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Build {
    #[default]
    Idle,
    Running,
    /// Running, and the vault changed since it read the entries: its apply asks for another.
    Dirty,
}

/// The cache outcome `queue_changed` and merges wait for, then the build that carries it (P7).
#[derive(Default)]
pub(super) enum Cache {
    #[default]
    Off,
    Awaited,
    /// The next build takes it; `None` for a file rejected or missing.
    Landed(Option<Decoded>),
    /// A build carries it; its apply ends the wait.
    Carried,
}

/// The diff of two entry snapshots; `run` it off the foreground.
pub struct BuildJob {
    /// The seq counter when the entries were read.
    seq: u64,
    last: Option<Arc<[Entry]>>,
    entries: Arc<[Entry]>,
    cache: Option<Decoded>,
}

/// A build's result, for `LiveIndex::apply_build`.
pub struct Built {
    seq: u64,
    entries: Arc<[Entry]>,
    change: Change,
    seed: Option<Seed>,
}

/// A note set built off the foreground, with what `apply_build` queues and prunes by.
pub(super) struct Layout {
    pub(super) notes: NoteSet,
    /// Every note's id and stamp, in vault order.
    stamps: Vec<(EntryId, Stamp)>,
    live: HashSet<EntryId>,
}

pub(super) struct Applied {
    /// The vault changed while the build ran: the turn's end starts another.
    pub(super) dirty: bool,
    /// The note set changed: names and link resolution may differ.
    pub(super) layout: bool,
    /// The build carried the cache's seed.
    pub(super) seeded: bool,
    pub(super) retired: Retired,
}

/// The previous note set, entries and live set, and the words a merge replaced; free them off
/// the foreground.
#[derive(Default)]
#[must_use = "free it off the foreground"]
pub struct Retired {
    last: Option<Arc<[Entry]>>,
    notes: Option<(NoteSet, HashSet<EntryId>)>,
    words: Option<RetiredWords>,
}

impl BuildJob {
    /// # Panics
    ///
    /// If the decoded words break an invariant the store keeps.
    #[must_use]
    pub fn run(self) -> Built {
        let change = Change::between(self.last.as_deref(), &self.entries);
        let seed = self.cache.map(|decoded| {
            let Change::Layout(layout) = &change else {
                panic!("a build carrying the cache has no last entries, so it is a layout");
            };
            Seed::build(decoded, notes(&self.entries), &layout.notes)
        });
        Built {
            seq: self.seq,
            entries: self.entries,
            change,
            seed,
        }
    }
}

impl Cache {
    pub(super) fn waits(&self) -> bool {
        !matches!(self, Self::Off)
    }
}

impl Retired {
    /// With the words a merge replaced.
    pub(super) fn with_words(self, words: Option<RetiredWords>) -> Self {
        Self { words, ..self }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.last.is_none() && self.notes.is_none() && self.words.is_none()
    }
}

impl Layout {
    pub(super) fn new(entries: &[Entry]) -> Self {
        let named = files(entries)
            .map(|entry| {
                let path = entry.path.as_path().to_string_lossy();
                (entry.id, path, entry.kind, entry.mtime)
            })
            .collect();
        let stamps: Vec<(EntryId, Stamp)> = notes(entries)
            .map(|entry| (entry.id, entry.stamp()))
            .collect();
        let live = stamps.iter().map(|(id, _)| *id).collect();
        Self {
            notes: NoteSet::build(named),
            stamps,
            live,
        }
    }
}

impl LiveIndex {
    /// Holds every queue and merge until `Turn::cached` lands the cache outcome (P7). Call it
    /// before the first turn.
    pub fn await_cache(&mut self) {
        self.cache = Cache::Awaited;
    }

    /// The cache outcome landed: the next build carries it.
    ///
    /// # Panics
    /// If no outcome is awaited.
    pub(super) fn cached(&mut self, outcome: Outcome) {
        assert!(
            matches!(self.cache, Cache::Awaited),
            "a cache outcome lands once, after `await_cache`"
        );
        self.cache = Cache::Landed(match outcome {
            Outcome::Decoded(decoded) => Some(decoded),
            Outcome::Rejected(_) | Outcome::Missing => None,
        });
    }

    /// Vault entries changed. `None` while a build runs; that build's apply then returns `dirty`.
    #[must_use]
    pub(super) fn start_build(&mut self, entries: Arc<[Entry]>) -> Option<BuildJob> {
        // Dropping the build in flight instead would starve the index under a stream of changes.
        if self.build != Build::Idle {
            self.build = Build::Dirty;
            return None;
        }
        self.build = Build::Running;
        let (last, cache) = match mem::take(&mut self.cache) {
            // No `last`: the wait queued nothing, so this build queues every note.
            Cache::Landed(cache) => {
                self.cache = Cache::Carried;
                (None, cache)
            }
            other => {
                self.cache = other;
                (self.last.clone(), None)
            }
        };
        Some(BuildJob {
            seq: self.next_seq,
            last,
            entries,
            cache,
        })
    }

    /// Applies a build and queues each changed note; `vault` is the live one, to retry forgotten notes.
    pub(super) fn apply_build(&mut self, built: Built, vault: &Vault) -> Applied {
        let Built {
            seq,
            entries,
            change,
            seed,
        } = built;
        let dirty = mem::take(&mut self.build) == Build::Dirty;
        let seeded = seed.is_some();
        let last = self.last.replace(entries);
        if matches!(self.cache, Cache::Carried) {
            self.cache = Cache::Off;
        }
        let retired_notes = match change {
            Change::Layout(layout) => Some(self.apply_notes(seq, *layout, seed)),
            Change::Stamps(moved) => {
                self.queue_changed(seq, moved);
                None
            }
        };
        // After the seed, so a rebuild asked during the wait clears what it seeded.
        if !self.cache.waits() && mem::take(&mut self.rebuild_due) {
            self.rebuild();
        }
        self.retry_forgotten(seq, vault);
        Applied {
            dirty,
            layout: retired_notes.is_some(),
            seeded,
            retired: Retired {
                last,
                notes: retired_notes,
                words: None,
            },
        }
    }

    /// Swaps in a build's notes, dropping what it no longer lists; the previous set and the live set.
    fn apply_notes(
        &mut self,
        seq: u64,
        layout: Layout,
        seed: Option<Seed>,
    ) -> (NoteSet, HashSet<EntryId>) {
        let Layout {
            notes,
            stamps,
            live,
        } = layout;
        self.note_sets += 1;
        self.revision += 1;
        let previous = self.index.set_notes(notes);
        self.indexed.retain(|id, _| live.contains(id));
        self.latest.retain(|id, _| live.contains(id));
        self.queued.retain(|id, _| live.contains(id));
        self.forgotten.retain(|id| live.contains(id));
        // A load in flight for a note gone is not pending; it keeps its slot until it lands.
        for load in &mut self.loading {
            load.live &= live.contains(&load.id);
        }
        if let Some(mut seed) = seed {
            self.cache_revision = (!seed.pruned).then_some(self.revision);
            self.scrub = Some(mem::take(&mut seed.scrub));
            for (id, stamp) in self.index.seed(seed) {
                // A save since launch holds a newer stamp than the cache's, or must reload (P6).
                if !self.latest.contains_key(&id) {
                    self.indexed.insert(id, stamp);
                }
            }
        }
        // Reserved for the notes not yet indexed, so no insert rehashes (open-first spec, O7).
        let missing = stamps.len().saturating_sub(self.indexed.len());
        self.indexed.reserve(missing);
        self.latest.reserve(missing);
        self.queued.reserve(missing);
        self.queue.reserve(missing);
        self.queue_changed(seq, stamps);
        (previous, live)
    }

    /// Queues each forgotten note still in the vault; after a `Layout`, `queue_changed` finds it queued already.
    fn retry_forgotten(&mut self, seq: u64, vault: &Vault) {
        // The live vault, not `last`: it finds an id in O(1), and the snapshot has no index by id.
        let retry = self
            .forgotten
            .iter()
            .filter_map(|&id| vault.entry(id))
            .map(|entry| (entry.id, entry.stamp()))
            .collect();
        self.queue_changed(seq, retry);
    }

    /// Queues each note whose stamp is not the one indexed; `seq` is the counter at the build's read.
    pub(super) fn queue_changed(&mut self, seq: u64, stamps: Vec<(EntryId, Stamp)>) {
        if self.cache.waits() {
            return;
        }
        for (id, stamp) in stamps {
            // A save since the build read the vault holds a newer stamp than the build's.
            let current = self.indexed.get(&id).is_some_and(|&indexed| {
                indexed == stamp || self.latest.get(&id).is_some_and(|&latest| latest > seq)
            });
            if !current {
                self.indexed.insert(id, stamp);
                self.queue_load(id);
            }
        }
    }
}
