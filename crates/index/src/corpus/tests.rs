use std::collections::HashMap;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

use inkra_vault::{EntryId, Mtime, Stamp};

use super::{CHUNK, Corpus, CorpusSnapshot, SLOT_BYTES, Slot};
use crate::test_support::{entries, mtime, vault};

fn ids(files: &[&str]) -> Vec<EntryId> {
    let (_fs, vault) = vault(files);
    entries(&vault).into_iter().map(|(id, ..)| id).collect()
}

fn stamps(first: u64, len: usize) -> Vec<Mtime> {
    (first..).take(len).map(mtime).collect()
}

fn stamp(n: u64) -> Stamp {
    Stamp {
        mtime: mtime(n),
        size: n,
    }
}

fn written(snapshot: &CorpusSnapshot) -> Vec<Option<Stamp>> {
    (0..=snapshot.len()).map(|at| snapshot.stamp(at)).collect()
}

fn corpus(ids: &[EntryId]) -> Corpus {
    let mut corpus = Corpus::default();
    let _ = corpus.reorder(ids.into(), &stamps(0, ids.len()), |_| usize::MAX);
    corpus
}

fn modified(snapshot: &CorpusSnapshot) -> Vec<Option<Mtime>> {
    (0..=snapshot.len())
        .map(|at| snapshot.modified(at))
        .collect()
}

/// `order` of `old`'s notes, as `NoteIndex` gives it to `reorder`.
fn order_in(old: &[EntryId]) -> impl Fn(EntryId) -> usize {
    move |id| old.iter().position(|&x| x == id).unwrap_or(usize::MAX)
}

/// `len` notes, each slot stamped with its own position.
fn numbered(len: usize) -> (Vec<EntryId>, Corpus) {
    let files: Vec<String> = (0..len).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let ids = ids(&files);
    let mut corpus = corpus(&ids);
    for at in 0..len {
        corpus.write(at, stamp(at as u64));
    }
    (ids, corpus)
}

/// Each loaded note of `range` and its stamp, in vault order.
fn listed(snapshot: &CorpusSnapshot, range: Range<usize>) -> Vec<(EntryId, Stamp)> {
    range
        .filter_map(|at| Some((snapshot.ids[at], snapshot.stamp(at)?)))
        .collect()
}

fn loaded(snapshot: &CorpusSnapshot) -> Vec<(EntryId, Stamp)> {
    listed(snapshot, 0..snapshot.len())
}

fn expected(ids: &[EntryId], range: Range<usize>) -> Vec<(EntryId, Stamp)> {
    range.map(|at| (ids[at], stamp(at as u64))).collect()
}

#[test]
fn heap_bytes_counts_every_slot() {
    let ids = ids(&["a.md", "b.md", "c.md"]);
    let mut corpus = corpus(&ids);
    assert_eq!(corpus.snapshot().heap_bytes(), 3 * SLOT_BYTES);

    corpus.write(0, stamp(5));
    corpus.write(2, stamp(5));
    corpus.clear(2);
    assert_eq!(corpus.snapshot().heap_bytes(), 3 * SLOT_BYTES);
}

#[test]
fn a_write_past_the_end_is_ignored() {
    let ids = ids(&["a.md"]);
    let mut corpus = corpus(&ids);
    corpus.write(1, stamp(0));
    corpus.write(usize::MAX, stamp(0));
    assert_eq!(loaded(&corpus.snapshot()), []);
    assert_eq!(corpus.snapshot().heap_bytes(), SLOT_BYTES);
}

#[test]
fn a_held_snapshot_never_sees_a_later_write() {
    let ids = ids(&["a.md", "b.md"]);
    let mut corpus = corpus(&ids);
    corpus.write(0, stamp(10));
    let held = corpus.snapshot();

    corpus.write(0, stamp(11));
    corpus.write(1, stamp(12));

    assert_eq!(loaded(&held), [(ids[0], stamp(10))]);
    assert_eq!(
        loaded(&corpus.snapshot()),
        [(ids[0], stamp(11)), (ids[1], stamp(12))]
    );

    corpus.clear(0);
    let _ = corpus.reorder(vec![ids[1], ids[0]].into(), &stamps(20, 2), order_in(&ids));
    assert_eq!(modified(&held), [Some(mtime(10)), Some(mtime(1)), None]);
}

#[test]
fn a_write_sets_the_mtime_with_the_stamp() {
    let ids = ids(&["a.md", "b.md"]);
    let mut corpus = corpus(&ids);
    corpus.write(1, stamp(5));
    assert_eq!(
        modified(&corpus.snapshot()),
        [Some(mtime(0)), Some(mtime(5)), None]
    );

    corpus.write(1, stamp(6));
    corpus.write(2, stamp(7));
    assert_eq!(
        modified(&corpus.snapshot()),
        [Some(mtime(0)), Some(mtime(6)), None]
    );
    assert_eq!(loaded(&corpus.snapshot()), [(ids[1], stamp(6))]);
}

#[test]
fn a_remove_keeps_the_mtime() {
    let (ids, mut corpus) = numbered(CHUNK + 1);
    corpus.write(1, stamp(5));
    let held = corpus.snapshot();

    corpus.clear(1);
    corpus.clear(1);
    corpus.clear(CHUNK + 1);

    let now = corpus.snapshot();
    assert_eq!(now.modified(1), Some(mtime(5)));
    assert_eq!(listed(&now, 0..3), [(ids[0], stamp(0)), (ids[2], stamp(2))]);
    assert_eq!(now.heap_bytes(), held.heap_bytes());
    assert!(
        Arc::ptr_eq(&held.slots[1], &now.slots[1]),
        "a clear copies one chunk"
    );

    let held = corpus.snapshot();
    corpus.clear(1);
    assert!(
        Arc::ptr_eq(&held.slots, &corpus.snapshot().slots),
        "clearing a slot with no stamp copies nothing"
    );
}

#[test]
fn a_note_not_loaded_takes_the_build_s_mtime() {
    let all = ids(&["a.md", "b.md", "c.md"]);
    let (a, b, c) = (all[0], all[1], all[2]);
    let mut corpus = corpus(&[a, b]);
    assert_eq!(
        modified(&corpus.snapshot()),
        [Some(mtime(0)), Some(mtime(1)), None]
    );

    corpus.write(0, stamp(5));
    corpus.clear(0);
    let _ = corpus.reorder(vec![c, b, a].into(), &stamps(20, 3), order_in(&[a, b]));

    assert_eq!(
        modified(&corpus.snapshot()),
        [Some(mtime(20)), Some(mtime(21)), Some(mtime(22)), None]
    );
}

#[test]
fn a_slot_keeps_its_stamp_s_mtime_across_a_reorder() {
    let (ids, mut corpus) = numbered(CHUNK + 2);
    corpus.write(CHUNK + 1, stamp(5));
    let kept: Vec<EntryId> = ids.iter().rev().copied().collect();

    let _ = corpus.reorder(
        kept.clone().into(),
        &stamps(1000, kept.len()),
        order_in(&ids),
    );

    let now = corpus.snapshot();
    assert_eq!(now.modified(0), Some(mtime(5)), "the stamp written last");
    assert_eq!(
        now.modified(CHUNK + 1),
        Some(mtime(0)),
        "the stamp `numbered` wrote"
    );
    assert_eq!(listed(&now, 0..1), [(ids[CHUNK + 1], stamp(5))]);
}

#[test]
fn reorder_keeps_each_kept_note_s_stamp_and_drops_the_rest() {
    let all = ids(&["a.md", "b.md", "c.md", "d.md"]);
    let (a, b, c, d) = (all[0], all[1], all[2], all[3]);
    let mut corpus = corpus(&[a, b, c]);
    corpus.write(0, stamp(1));
    corpus.write(1, stamp(2));
    corpus.write(2, stamp(3));
    let held = corpus.snapshot();

    let old = [a, b, c];
    let previous = corpus.reorder(vec![d, c, a].into(), &stamps(10, 3), order_in(&old));

    assert_eq!(&*previous, &[a, b, c]);
    let now = corpus.snapshot();
    assert_eq!(now.len(), 3);
    assert_eq!(loaded(&now), [(c, stamp(3)), (a, stamp(1))]);
    assert_eq!(now.heap_bytes(), 3 * SLOT_BYTES);
    assert_eq!(loaded(&held).len(), 3, "a held snapshot keeps its order");
}

#[test]
fn a_snapshot_counts_its_slots() {
    let files: Vec<String> = (0..10).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let corpus = corpus(&ids(&files));
    let snapshot = corpus.snapshot();
    assert_eq!(snapshot.len(), 10);
    assert!(!snapshot.is_empty());
    assert!(Corpus::default().snapshot().is_empty());
}

#[test]
fn a_slot_costs_its_mtime_its_stamp_and_its_id() {
    assert_eq!(SLOT_BYTES, size_of::<Slot>() + size_of::<EntryId>());
    assert!(size_of::<Slot>() >= size_of::<Mtime>() + size_of::<Option<Stamp>>());
}

#[test]
fn a_write_under_a_held_snapshot_copies_one_chunk() {
    let (ids, mut corpus) = numbered(3 * CHUNK);
    let held = corpus.snapshot();

    corpus.write(CHUNK + 5, stamp(0));

    assert_eq!(loaded(&held), expected(&ids, 0..3 * CHUNK));
    let now = corpus.snapshot();
    let shared: Vec<bool> = held
        .slots
        .iter()
        .zip(now.slots.iter())
        .map(|(held, now)| Arc::ptr_eq(held, now))
        .collect();
    assert_eq!(shared, [true, false, true]);
    assert_eq!(
        listed(&now, CHUNK + 4..CHUNK + 6),
        [
            (ids[CHUNK + 4], stamp((CHUNK + 4) as u64)),
            (ids[CHUNK + 5], stamp(0))
        ]
    );
}

#[test]
fn every_slot_is_read_across_chunks() {
    for len in [2 * CHUNK, 2 * CHUNK + 3] {
        let (ids, corpus) = numbered(len);
        let snapshot = corpus.snapshot();
        assert_eq!(snapshot.slots.len(), len.div_ceil(CHUNK));
        assert_eq!(loaded(&snapshot), expected(&ids, 0..len));
        assert_eq!(snapshot.stamp(len), None, "past the end");
    }
    assert_eq!(written(&Corpus::default().snapshot()), [None]);
}

#[test]
fn reorder_moves_stamps_across_chunks() {
    let (ids, mut corpus) = numbered(2 * CHUNK + 3);
    let held = corpus.snapshot();
    let order: HashMap<EntryId, usize> = ids.iter().enumerate().map(|(at, &id)| (id, at)).collect();
    let kept: Vec<EntryId> = ids[1..].iter().rev().copied().collect();

    let _ = corpus.reorder(kept.clone().into(), &stamps(0, kept.len()), |id| order[&id]);

    let now = corpus.snapshot();
    let moved: Vec<(EntryId, Stamp)> = kept
        .iter()
        .map(|id| (*id, stamp(order[id] as u64)))
        .collect();
    assert_eq!(loaded(&now), moved);
    assert_eq!(now.heap_bytes(), kept.len() * SLOT_BYTES);
    assert_eq!(loaded(&held), expected(&ids, 0..ids.len()));
}

#[test]
fn a_stamp_goes_through_a_reorder_and_not_a_clear() {
    let ids = ids(&["a.md", "b.md", "c.md"]);
    let mut corpus = corpus(&ids);
    corpus.write(0, stamp(5));
    corpus.write(1, stamp(6));
    corpus.clear(1);

    let now = corpus.snapshot();
    assert_eq!(written(&now), [Some(stamp(5)), None, None, None]);
    assert_eq!(now.modified(1), Some(mtime(6)), "a clear keeps the mtime");

    let _ = corpus.reorder(vec![ids[2], ids[0]].into(), &stamps(20, 2), order_in(&ids));
    assert_eq!(written(&corpus.snapshot()), [None, Some(stamp(5)), None]);
}
