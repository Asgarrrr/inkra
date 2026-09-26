use std::collections::HashMap;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use super::{CHUNK, Corpus, CorpusSnapshot, SLOT_BYTES, TEXT_HEADER_BYTES};
use crate::test_support::{entries, texts, vault};

fn ids(files: &[&str]) -> Vec<EntryId> {
    let (_fs, vault) = vault(files);
    entries(&vault).into_iter().map(|(id, _, _)| id).collect()
}

fn corpus(ids: &[EntryId]) -> Corpus {
    let mut corpus = Corpus::default();
    let _ = corpus.reorder(ids.into(), |_| usize::MAX);
    corpus
}

/// `len` notes, each slot holding its own position as text.
fn numbered(len: usize) -> (Vec<EntryId>, Corpus) {
    let files: Vec<String> = (0..len).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let ids = ids(&files);
    let mut corpus = corpus(&ids);
    for at in 0..len {
        corpus.set(at, Some(at.to_string().into()));
    }
    (ids, corpus)
}

fn listed(snapshot: &CorpusSnapshot, range: Range<usize>) -> Vec<(EntryId, String)> {
    snapshot
        .notes(range)
        .map(|(id, text)| (id, text.to_owned()))
        .collect()
}

fn expected(ids: &[EntryId], range: Range<usize>) -> Vec<(EntryId, String)> {
    range.map(|at| (ids[at], at.to_string())).collect()
}

#[test]
fn heap_bytes_counts_every_slot_and_each_text() {
    let ids = ids(&["a.md", "b.md", "c.md"]);
    let mut corpus = corpus(&ids);
    assert_eq!(corpus.snapshot().heap_bytes(), 3 * SLOT_BYTES);

    corpus.set(0, Some("hello".into()));
    corpus.set(2, Some("été".into()));
    assert_eq!(
        corpus.snapshot().heap_bytes(),
        3 * SLOT_BYTES + 2 * TEXT_HEADER_BYTES + 5 + 5
    );

    corpus.set(0, Some("hi".into()));
    corpus.set(2, None);
    assert_eq!(
        corpus.snapshot().heap_bytes(),
        3 * SLOT_BYTES + TEXT_HEADER_BYTES + 2
    );
}

#[test]
fn a_write_past_the_end_is_ignored() {
    let ids = ids(&["a.md"]);
    let mut corpus = corpus(&ids);
    corpus.set(1, Some("x".into()));
    corpus.set(usize::MAX, Some("x".into()));
    assert_eq!(texts(&corpus.snapshot()), []);
    assert_eq!(corpus.snapshot().heap_bytes(), SLOT_BYTES);
}

#[test]
fn a_held_snapshot_never_sees_a_later_write() {
    let ids = ids(&["a.md", "b.md"]);
    let mut corpus = corpus(&ids);
    corpus.set(0, Some("old".into()));
    let held = corpus.snapshot();

    corpus.set(0, Some("new".into()));
    corpus.set(1, Some("b".into()));

    assert_eq!(texts(&held), [(ids[0], "old".to_owned())]);
    assert_eq!(held.heap_bytes(), 2 * SLOT_BYTES + TEXT_HEADER_BYTES + 3);
    assert_eq!(
        texts(&corpus.snapshot()),
        [(ids[0], "new".to_owned()), (ids[1], "b".to_owned())]
    );
}

#[test]
fn reorder_keeps_each_kept_note_s_text_and_drops_the_rest() {
    let all = ids(&["a.md", "b.md", "c.md", "d.md"]);
    let (a, b, c, d) = (all[0], all[1], all[2], all[3]);
    let mut corpus = corpus(&[a, b, c]);
    corpus.set(0, Some("A".into()));
    corpus.set(1, Some("BB".into()));
    corpus.set(2, Some("CCC".into()));
    let held = corpus.snapshot();

    let old = [a, b, c];
    let (previous, retired) = corpus.reorder(vec![d, c, a].into(), |id| {
        old.iter().position(|&x| x == id).unwrap_or(usize::MAX)
    });

    assert_eq!(&*previous, &[a, b, c]);
    assert_eq!(
        retired,
        [Arc::<str>::from("BB")],
        "the text of the note gone"
    );
    let now = corpus.snapshot();
    assert_eq!(now.len(), 3);
    assert_eq!(texts(&now), [(c, "CCC".to_owned()), (a, "A".to_owned())]);
    assert_eq!(now.heap_bytes(), 3 * SLOT_BYTES + 2 * TEXT_HEADER_BYTES + 4);
    assert_eq!(texts(&held).len(), 3, "a held snapshot keeps its order");
}

#[test]
fn a_snapshot_splits_into_ranges_over_its_slots() {
    let files: Vec<String> = (0..10).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let corpus = corpus(&ids(&files));
    let snapshot = corpus.snapshot();
    assert_eq!(snapshot.len(), 10);
    assert!(!snapshot.is_empty());
    assert_eq!(snapshot.chunks(3), [0..3, 3..6, 6..10]);
    assert!(Corpus::default().snapshot().is_empty());
    assert!(Corpus::default().snapshot().chunks(4).is_empty());
}

#[test]
fn notes_lists_the_loaded_notes_of_a_range() {
    let ids = ids(&["a.md", "b.md", "c.md"]);
    let mut corpus = corpus(&ids);
    corpus.set(0, Some("a".into()));
    corpus.set(2, Some("c".into()));
    let snapshot = corpus.snapshot();
    let found: Vec<(EntryId, &str)> = snapshot.notes(1..3).collect();
    assert_eq!(found, [(ids[2], "c")]);
}

#[test]
fn a_slot_costs_its_text_pointer_and_its_id() {
    assert_eq!(
        SLOT_BYTES,
        size_of::<Option<Arc<str>>>() + size_of::<EntryId>()
    );
}

#[test]
fn a_loaded_text_costs_its_arc_s_two_counts() {
    assert_eq!(TEXT_HEADER_BYTES, 2 * size_of::<usize>());
}

#[test]
fn a_write_under_a_held_snapshot_copies_one_chunk() {
    let (ids, mut corpus) = numbered(3 * CHUNK);
    let held = corpus.snapshot();

    corpus.set(CHUNK + 5, Some("new".into()));

    assert_eq!(texts(&held), expected(&ids, 0..3 * CHUNK));
    let now = corpus.snapshot();
    let shared: Vec<bool> = held
        .texts
        .iter()
        .zip(now.texts.iter())
        .map(|(held, now)| Arc::ptr_eq(held, now))
        .collect();
    assert_eq!(shared, [true, false, true]);
    assert_eq!(
        listed(&now, CHUNK + 4..CHUNK + 6),
        [
            (ids[CHUNK + 4], (CHUNK + 4).to_string()),
            (ids[CHUNK + 5], "new".to_owned())
        ]
    );
}

#[test]
fn notes_crosses_chunk_boundaries() {
    let (ids, corpus) = numbered(2 * CHUNK + 3);
    let snapshot = corpus.snapshot();
    for range in [
        CHUNK / 2..2 * CHUNK + 1,
        CHUNK..2 * CHUNK,
        0..2 * CHUNK + 3,
        2 * CHUNK..2 * CHUNK + 3,
        CHUNK + 1..CHUNK + 1,
    ] {
        assert_eq!(listed(&snapshot, range.clone()), expected(&ids, range));
    }
}

#[test]
fn notes_reaches_the_end_of_whole_chunks() {
    let (ids, corpus) = numbered(2 * CHUNK);
    let snapshot = corpus.snapshot();
    assert_eq!(snapshot.texts.len(), 2);
    for range in [0..2 * CHUNK, CHUNK + 1..2 * CHUNK, 2 * CHUNK..2 * CHUNK] {
        assert_eq!(listed(&snapshot, range.clone()), expected(&ids, range));
    }
}

#[test]
fn notes_of_an_empty_corpus_is_empty() {
    assert_eq!(Corpus::default().snapshot().notes(0..0).count(), 0);
    assert_eq!(corpus(&[]).snapshot().notes(0..0).count(), 0);
}

#[test]
fn reorder_moves_texts_across_chunks() {
    let (ids, mut corpus) = numbered(2 * CHUNK + 3);
    let held = corpus.snapshot();
    let order: HashMap<EntryId, usize> = ids.iter().enumerate().map(|(at, &id)| (id, at)).collect();
    let kept: Vec<EntryId> = ids[1..].iter().rev().copied().collect();

    let (_, retired) = corpus.reorder(kept.clone().into(), |id| order[&id]);

    assert_eq!(retired, [Arc::<str>::from("0")]);
    let now = corpus.snapshot();
    let moved: Vec<(EntryId, String)> =
        kept.iter().map(|id| (*id, order[id].to_string())).collect();
    assert_eq!(texts(&now), moved);
    let bytes: usize = (1..ids.len()).map(|at| at.to_string().len()).sum();
    assert_eq!(
        now.heap_bytes(),
        kept.len() * (SLOT_BYTES + TEXT_HEADER_BYTES) + bytes
    );
    assert_eq!(texts(&held), expected(&ids, 0..ids.len()));
}
