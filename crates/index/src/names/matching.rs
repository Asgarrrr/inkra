use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::EntryId;

use super::query::NameQuery;
use super::{NamesSnapshot, score};

const CANCEL_EVERY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameMatch {
    pub id: EntryId,
    /// The path `positions` index: NFC, `/`-separated, `.md` stripped.
    pub path: Arc<str>,
    pub score: i32,
    /// Byte offsets into `path` of the matched chars, ascending.
    pub positions: Vec<usize>,
    rank: Rank,
}

/// Spec D2's order; smaller is better. `order` is unique, so the order is total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    score: Reverse<i32>,
    recent: usize,
    chars: usize,
    order: usize,
}

/// The best `limit` matches in `range`, best first. Empty when `cancel` is set.
#[must_use]
pub fn match_names(
    names: &NamesSnapshot,
    range: Range<usize>,
    query: &NameQuery,
    cancel: &AtomicBool,
    limit: usize,
) -> Vec<NameMatch> {
    let notes = names.candidates();
    let mut kept: BinaryHeap<Rank> = BinaryHeap::new();
    let mut table = Vec::new();
    for block in range.clone().step_by(CANCEL_EVERY) {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let end = (block + CANCEL_EVERY).min(range.end);
        for (order, note) in (block..).zip(&notes[block..end]) {
            let score = if query.is_empty() {
                query
                    .recent(note.id)
                    .filter(|_| query.showing != Some(note.id))
                    .map(|_| 0)
            } else {
                score::score(note, query, &mut table)
            };
            let Some(score) = score else {
                continue;
            };
            kept.push(Rank {
                score: Reverse(score),
                recent: query.recent(note.id).unwrap_or(usize::MAX),
                chars: note.chars.len(),
                order,
            });
            if kept.len() > limit {
                kept.pop();
            }
        }
    }
    kept.into_sorted_vec()
        .into_iter()
        .map(|rank| {
            let note = &notes[rank.order];
            NameMatch {
                id: note.id,
                path: Arc::clone(&note.path),
                score: rank.score.0,
                positions: score::positions(note, query, &mut table),
                rank,
            }
        })
        .collect()
}

/// Joins the ranges' results; equals one `match_names` over their union.
#[must_use]
pub fn merge_names(chunks: Vec<Vec<NameMatch>>, limit: usize) -> Vec<NameMatch> {
    let mut all: Vec<NameMatch> = chunks.into_iter().flatten().collect();
    all.sort_unstable_by_key(|found| found.rank);
    all.truncate(limit);
    all
}
