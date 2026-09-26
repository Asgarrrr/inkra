//! When the word index merges (smart quick open spec S2).

/// Fresh notes that make a merge due, or `1 / FRESH_RATIO` of the live ones if more; bench-set.
pub(crate) const FRESH_MIN: usize = 256;
pub(super) const FRESH_RATIO: usize = 8;
/// The dead share of the segment's notes that makes a merge due (Lucene's `deletesPctAllowed`).
pub(super) const DEAD_PERCENT: usize = 20;

/// When `NoteIndex::words_merge` starts a merge; never while one runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeWhen {
    /// When the policy says one is due.
    Due,
    /// When one is due or any note is fresh, so an idle index's lookups scan no fresh tier.
    Settle,
}

/// Whether `fresh` of the `live` notes, or `dead` of the segment's `notes`, make a merge due.
pub(super) fn is_due(fresh: usize, live: usize, dead: usize, notes: usize) -> bool {
    fresh >= FRESH_MIN.max(live / FRESH_RATIO) || (dead > 0 && dead * 100 >= notes * DEAD_PERCENT)
}
