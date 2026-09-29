//! A `NoteIndex` kept current from five inputs: vault entries changed, a note saved, a load
//! slot free, a load or save landed, a foreground turn ended (its merge landing in `merged`).
//! It reads no file and no clock and spawns nothing: each input returns the work to run, and
//! the caller runs it where it wants.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use inkra_vault::{Entry, EntryId, Mtime};

use crate::index::NoteIndex;

mod builds;
mod diff;
mod loads;
mod startup;

pub use builds::{Applied, BuildJob, Built, Retired};
pub use loads::{Load, Ticket};
pub use startup::{Merge, Merged, TurnEnd};

use builds::Build;
use loads::Loading;
use startup::Startup;

/// A `NoteIndex` kept current: which notes to load, which load is superseded, when the words merge.
#[derive(Default)]
pub struct LiveIndex {
    index: NoteIndex,
    /// The mtime each note was indexed at, or is queued to be.
    indexed: HashMap<EntryId, Mtime>,
    /// One counter orders loads and saves.
    next_seq: u64,
    /// The latest seq minted per note: only a ticket carrying it acts; an older one is superseded.
    latest: HashMap<EntryId, u64>,
    queue: VecDeque<(EntryId, u64)>,
    /// The seq of each note's live queue entry; an older entry for it is skipped.
    queued: HashMap<EntryId, u64>,
    /// Notes a failed load forgot; every apply queues them again, since an `Mtimes` change may not list them.
    forgotten: HashSet<EntryId>,
    /// Each load started and not landed yet.
    loading: Vec<Loading>,
    build: Build,
    /// The vault entries the last applied build was read from; `None` until the first applies.
    last: Option<Arc<[Entry]>>,
    /// Note sets applied; a failed load compares it to retry after a build that may have moved its note.
    note_sets: u64,
    startup: Startup,
}

impl LiveIndex {
    #[must_use]
    pub fn index(&self) -> &NoteIndex {
        &self.index
    }

    /// `(loaded, total)`: the notes of the last build, less those queued or loading.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
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

    /// Until the first build applies, there are no names to list.
    #[must_use]
    pub fn awaits_first_build(&self) -> bool {
        self.last.is_none()
    }

    /// True before the first build applies or while loads are pending; a rebuild alone is not.
    #[must_use]
    pub fn is_building(&self) -> bool {
        let (loaded, total) = self.progress();
        self.awaits_first_build() || loaded < total
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod startup_tests;

#[cfg(test)]
mod model_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod diff_tests;
