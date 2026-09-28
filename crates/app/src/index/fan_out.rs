//! One query over the index, spread across cores: the lookup, a scan per range, the merge.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{BackgroundExecutor, Task};
use inkra_index::{IndexSnapshot, Lookup, NoteQuery};

/// A scan of one range of the snapshot, for this query's lookup.
pub(crate) type Scan<T> = fn(&IndexSnapshot, &Lookup, Range<usize>, &NoteQuery, &AtomicBool) -> T;

/// The ranges' results, in range order, as one.
pub(crate) type Merge<T, M> = fn(&IndexSnapshot, Vec<T>, &NoteQuery) -> M;

/// `scan` over each range, merged; `None` once `cancel` is set, so nothing stale applies.
pub(crate) async fn fan_out<T: Send + 'static, M: Send + 'static>(
    executor: BackgroundExecutor,
    snapshot: IndexSnapshot,
    query: Arc<NoteQuery>,
    cancel: Arc<AtomicBool>,
    scan: Scan<T>,
    merge: Merge<T, M>,
) -> Option<M> {
    let lookup = {
        let (snapshot, query, cancel) = (snapshot.clone(), Arc::clone(&query), Arc::clone(&cancel));
        Arc::new(
            executor
                .spawn(async move { snapshot.lookup(&query, &cancel) })
                .await?,
        )
    };
    // One range per core: 1× wins on the costliest queries (find bench).
    let scans: Vec<Task<T>> = snapshot
        .chunks(executor.num_cpus())
        .into_iter()
        .map(|range| {
            let (snapshot, lookup, query, cancel) = (
                snapshot.clone(),
                Arc::clone(&lookup),
                Arc::clone(&query),
                Arc::clone(&cancel),
            );
            executor.spawn(async move { scan(&snapshot, &lookup, range, &query, &cancel) })
        })
        .collect();
    let mut found = Vec::with_capacity(scans.len());
    for scan in scans {
        found.push(scan.await);
    }
    executor
        .spawn(async move {
            let merged = merge(&snapshot, found, &query);
            // A cancelled range returns nothing: the merge of the others is not the query's answer.
            (!cancel.load(Ordering::Relaxed)).then_some(merged)
        })
        .await
}
