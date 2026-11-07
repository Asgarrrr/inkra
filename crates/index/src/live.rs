//! A `NoteIndex` kept current from five inputs: vault entries changed, a note saved, a load
//! slot free, a load or save landed, a foreground turn ended (its merge landing in `merged`).
//! It reads no file and no clock and spawns nothing: each input returns the work to run, and
//! the caller runs it where it wants.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use inkra_vault::{Entry, EntryId, Stamp};

use crate::index::NoteIndex;

mod builds;
mod diff;
mod loads;
mod startup;
mod turn;

pub use builds::{BuildJob, Built, Retired};
pub use loads::{Load, Ticket};
pub use startup::{Merge, Merged};
pub use turn::{Effects, Turn};

use builds::{Applied, Build, Cache};
use loads::Loading;
use startup::{Startup, TurnEnd};

/// What the index can answer now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No build has applied: there are no names to list.
    Scanning,
    /// The last build's notes less those queued or loading; `loaded < total`.
    Indexing { loaded: usize, total: usize },
    /// Every note of the last build is loaded; a rebuild in flight does not count.
    Ready,
}

impl Status {
    #[must_use]
    pub fn is_ready(self) -> bool {
        self == Self::Ready
    }
}

/// A `NoteIndex` kept current: which notes to load, which load is superseded, when the words merge.
pub struct LiveIndex {
    index: NoteIndex,
    /// The stamp each note was indexed at, or is queued to be.
    indexed: HashMap<EntryId, Stamp>,
    /// One counter orders loads and saves.
    next_seq: u64,
    /// The latest seq minted per note: only a ticket carrying it acts; an older one is superseded.
    latest: HashMap<EntryId, u64>,
    queue: VecDeque<(EntryId, u64)>,
    /// The seq of each note's live queue entry; an older entry for it is skipped.
    queued: HashMap<EntryId, u64>,
    /// Notes a failed load forgot; every apply queues them again, since a `Stamps` change may not list them.
    forgotten: HashSet<EntryId>,
    /// Each load started and not landed yet.
    loading: Vec<Loading>,
    build: Build,
    cache: Cache,
    /// A rebuild asked while the cache waits; the build that ends the wait runs it.
    rebuild_due: bool,
    /// The vault entries the last applied build was read from; `None` until the first applies.
    last: Option<Arc<[Entry]>>,
    /// Note sets applied; a failed load compares it to retry after a build that may have moved its note.
    note_sets: u64,
    revision: u64,
    /// The revision the cache file holds: the seed's, unless it pruned a note.
    cache_revision: Option<u64>,
    /// The seeded notes the turn that emits `indexed` hands to the scrub (P16).
    scrub: Option<Vec<EntryId>>,
    startup: Startup,
    /// How many loads may run at once.
    slots: usize,
}

// A getter added to this `impl` joins `Seen` in `live/model_tests.rs`, or D5's check misses it.
impl LiveIndex {
    /// `slots`: how many loads may run at once.
    ///
    /// # Panics
    /// If `slots` is 0: no note would ever load.
    #[must_use]
    pub fn new(slots: usize) -> Self {
        assert!(slots > 0, "a live index needs a load slot");
        Self {
            index: NoteIndex::default(),
            indexed: HashMap::new(),
            next_seq: 0,
            latest: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashMap::new(),
            forgotten: HashSet::new(),
            loading: Vec::new(),
            build: Build::default(),
            cache: Cache::default(),
            rebuild_due: false,
            last: None,
            note_sets: 0,
            revision: 0,
            cache_revision: None,
            scrub: None,
            startup: Startup::default(),
            slots,
        }
    }

    #[must_use]
    pub fn index(&self) -> &NoteIndex {
        &self.index
    }

    /// Moves when an answer may differ: a layout applied, a note's content inserted or forgotten.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The revision the cache file holds; `None` when it holds none or more than the index.
    #[must_use]
    pub fn cache_revision(&self) -> Option<u64> {
        self.cache_revision
    }

    /// `(loaded, total)`: the notes of the last build, less those queued or loading; the cache
    /// wait counts as one note pending, so an empty vault is not `Ready` before it ends.
    #[must_use]
    pub(crate) fn progress(&self) -> (usize, usize) {
        let wait = usize::from(self.cache.waits());
        let total = self.index.names().len() + wait;
        // A note loading and queued again is one note pending.
        let loading = self
            .loading
            .iter()
            .filter(|load| load.live && !self.queued.contains_key(&load.id))
            .count();
        let pending = self.queued.len() + loading + wait;
        (total.saturating_sub(pending), total)
    }

    /// The index's state now; `Scanning` never comes back once a build applies.
    #[must_use]
    pub fn status(&self) -> Status {
        if self.awaits_first_build() {
            return Status::Scanning;
        }
        let (loaded, total) = self.progress();
        if loaded < total {
            Status::Indexing { loaded, total }
        } else {
            Status::Ready
        }
    }

    /// Until the first build applies, there are no names to list.
    #[must_use]
    fn awaits_first_build(&self) -> bool {
        self.last.is_none()
    }

    /// True before the first build applies or while loads are pending; a rebuild alone is not.
    #[must_use]
    fn is_building(&self) -> bool {
        !self.status().is_ready()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod startup_tests;

#[cfg(test)]
mod turn_tests;

#[cfg(test)]
mod model_tests;

#[cfg(test)]
mod diff_tests;

#[cfg(test)]
mod cache_tests;
