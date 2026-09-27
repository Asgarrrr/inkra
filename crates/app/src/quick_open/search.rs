//! The background part of a query: the lookup, `find_notes` per range, the merge.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{BackgroundExecutor, Task};
use inkra_index::{IndexSnapshot, NoteMatch, NoteQuery, Ranked, find_notes, merge_notes};

/// Zed's file finder lists 100 too.
const LIMIT: usize = 100;

/// The best `LIMIT` notes for `query`; `None` once `cancel` is set, so nothing stale applies.
pub(super) async fn found(
    executor: BackgroundExecutor,
    snapshot: IndexSnapshot,
    query: Arc<NoteQuery>,
    cancel: Arc<AtomicBool>,
) -> Option<Vec<NoteMatch>> {
    let lookup = {
        let (snapshot, query, cancel) = (snapshot.clone(), Arc::clone(&query), Arc::clone(&cancel));
        Arc::new(
            executor
                .spawn(async move { snapshot.lookup(&query, &cancel) })
                .await?,
        )
    };
    // One range per core: 1× wins on the costliest queries (find bench).
    let scans: Vec<Task<Vec<Ranked>>> = snapshot
        .chunks(executor.num_cpus())
        .into_iter()
        .map(|range| {
            let (snapshot, lookup, query, cancel) = (
                snapshot.clone(),
                Arc::clone(&lookup),
                Arc::clone(&query),
                Arc::clone(&cancel),
            );
            executor
                .spawn(async move { find_notes(&snapshot, &lookup, range, &query, &cancel, LIMIT) })
        })
        .collect();
    let mut found = Vec::with_capacity(scans.len());
    for scan in scans {
        found.push(scan.await);
    }
    executor
        .spawn(async move {
            let found = merge_notes(&snapshot, found, &query, LIMIT);
            // A cancelled range returns no notes: the merge of the others is not the query's top.
            (!cancel.load(Ordering::Relaxed)).then_some(found)
        })
        .await
}
