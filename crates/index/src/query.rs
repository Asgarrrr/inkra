//! One query over a whole snapshot: the lookup, one scan per job, the merge.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::{Job, Jobs};

use crate::find::{find_in, merge_found};
use crate::index::IndexSnapshot;
use crate::search::search_in;
use crate::words::{Lookup, NoteQuery};
use crate::{NoteMatch, Searched};

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

    /// Vault search's hits: the first `limit` word starts, grouped by note in vault order.
    /// `None` once `cancel` is set.
    pub async fn search(
        &self,
        query: &NoteQuery,
        limit: usize,
        jobs: &dyn Jobs,
        cancel: &AtomicBool,
    ) -> Option<Searched> {
        let scan = |lookup: &Lookup, range| search_in(self, lookup, range, query, cancel, limit);
        let merge = |found| Searched::merge(found, limit);
        fan_out(self, query, jobs, cancel, scan, merge).await
    }
}

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
