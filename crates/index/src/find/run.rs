use std::collections::BinaryHeap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::rank::{Rank, rank};
use super::{NoteMatch, Ranked, highlight};
use crate::index::IndexSnapshot;
use crate::words::{Lookup, NoteQuery};

const CANCEL_EVERY: usize = 1024;

/// The best `limit` notes of `range`, for `merge_notes` to order. Empty when `cancel` is set.
#[must_use]
pub fn find_notes(
    snapshot: &IndexSnapshot,
    lookup: &Lookup,
    range: Range<usize>,
    query: &NoteQuery,
    cancel: &AtomicBool,
    limit: usize,
) -> Vec<Ranked> {
    let mut kept: BinaryHeap<Rank> =
        BinaryHeap::with_capacity(limit.saturating_add(1).min(range.len()));
    for block in range.clone().step_by(CANCEL_EVERY) {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        for order in block..(block + CANCEL_EVERY).min(range.end) {
            let Some(rank) = rank(snapshot, lookup, query, order) else {
                continue;
            };
            kept.push(rank);
            if kept.len() > limit {
                kept.pop();
            }
        }
    }
    kept.into_iter().map(Ranked).collect()
}

/// Best first, equal to one range over the union; pass the `snapshot` and `query` `find_notes` read.
#[must_use]
pub fn merge_notes(
    snapshot: &IndexSnapshot,
    chunks: Vec<Vec<Ranked>>,
    query: &NoteQuery,
    limit: usize,
) -> Vec<NoteMatch> {
    let mut top: Vec<Rank> = chunks
        .into_iter()
        .flatten()
        .map(|Ranked(rank)| rank)
        .collect();
    if top.len() > limit {
        top.select_nth_unstable(limit);
        top.truncate(limit);
    }
    top.sort_unstable();
    let names = snapshot.names();
    top.into_iter()
        .map(|rank| NoteMatch {
            id: names.id(rank.order),
            path: Arc::clone(names.path(rank.order)),
            group: rank.group,
            name_highlights: highlight::name(names, rank.order, query),
            rank,
        })
        .collect()
}
