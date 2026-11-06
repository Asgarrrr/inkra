//! One query over a whole snapshot: the lookup, one scan per job, the merge.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::{Job, Jobs};

use crate::find::{find_in, merge_found};
use crate::index::IndexSnapshot;
use crate::words::{Lookup, NoteQuery};
use crate::{Candidate, NoteMatch};

impl IndexSnapshot {
    /// Quick open's rows: the best `limit` notes, best first. `None` once `cancel` is set.
    pub async fn find_notes(
        &self,
        query: &NoteQuery,
        limit: usize,
        jobs: &dyn Jobs,
        cancel: &AtomicBool,
    ) -> Option<Vec<NoteMatch>> {
        let scan = |lookup: &Lookup, range| find_in(self, lookup, range, query, cancel, limit);
        let merge = |found| merge_found(self, found, query, limit);
        fan_out(self, query, jobs, cancel, scan, merge).await
    }

    /// The note at `order` as a read: `None` while it is not loaded, or past the end.
    #[must_use]
    pub fn candidate(&self, order: usize) -> Option<Candidate> {
        let stamp = self.corpus().stamp(order)?;
        let names = self.names();
        Some(Candidate {
            id: names.id(order),
            path: names.file(order).clone(),
            stamp,
        })
    }

    /// Vault search's reads: each note whose words hold every term, in vault order, with the
    /// stamp of the text the words came from. `None` once `cancel` is set.
    ///
    /// # Panics
    /// If a note holds words but was never loaded.
    #[must_use]
    pub fn candidates(&self, query: &NoteQuery, cancel: &AtomicBool) -> Option<Vec<Candidate>> {
        let lookup = self.lookup(query, cancel)?;
        let names = self.names();
        let mut candidates = Vec::new();
        for block in (0..names.len()).step_by(CANCEL_EVERY) {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            for order in block..(block + CANCEL_EVERY).min(names.len()) {
                let id = names.id(order);
                let holds = lookup.ordinal(id).is_some_and(|ordinal| {
                    (0..lookup.term_count()).all(|term| lookup.hit(term, ordinal).is_some())
                });
                if !holds {
                    continue;
                }
                let Some(candidate) = self.candidate(order) else {
                    panic!("note {id:?} holds words but was never loaded");
                };
                candidates.push(candidate);
            }
        }
        Some(candidates)
    }
}

/// `candidates` checks its flag once per this many notes.
const CANCEL_EVERY: usize = 256;

/// `scan` over each of `jobs.parallelism()` ranges, merged in range order; `None` once `cancel` is set.
async fn fan_out<T: Send, M>(
    snapshot: &IndexSnapshot,
    query: &NoteQuery,
    jobs: &dyn Jobs,
    cancel: &AtomicBool,
    scan: impl Fn(&Lookup, Range<usize>) -> T + Sync,
    merge: impl FnOnce(Vec<T>) -> M,
) -> Option<M> {
    let lookup = snapshot.lookup(query, cancel)?;
    let ranges = snapshot.chunks(jobs.parallelism());
    // One slot per range, as Zed's `segment_results`: the jobs borrow, nothing is cloned.
    let mut slots: Vec<Option<T>> = ranges.iter().map(|_| None).collect();
    let (scan, lookup) = (&scan, &lookup);
    let scans: Vec<Job<'_>> = slots
        .iter_mut()
        .zip(ranges)
        .map(|(slot, range)| -> Job<'_> {
            Box::pin(async move {
                *slot = Some(scan(lookup, range));
            })
        })
        .collect();
    jobs.run(scans).await;
    let Some(found) = slots.into_iter().collect::<Option<Vec<T>>>() else {
        panic!("Jobs::run runs every job");
    };
    let merged = merge(found);
    // A cancelled range returns nothing: the merge of the others is not the query's answer.
    (!cancel.load(Ordering::Relaxed)).then_some(merged)
}

#[cfg(test)]
mod tests;
