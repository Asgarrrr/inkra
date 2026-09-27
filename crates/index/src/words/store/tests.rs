use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use inkra_vault::EntryId;

use super::{Words, WordsMerge, WordsSnapshot};
use crate::test_support::{id, vault};
use crate::words::NoteWords;
use crate::words::shards::SHARDS;
use crate::{NoteQuery, TermHit};

pub(super) type Entries = BTreeMap<(String, EntryId), u32>;

/// `n` note ids, in vault order.
pub(super) fn ids(n: usize) -> Vec<EntryId> {
    let files: Vec<String> = (0..n).map(|n| format!("n{n:03}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    files.iter().map(|file| id(&vault, file)).collect()
}

pub(super) fn set(words: &mut Words, id: EntryId, text: &str) {
    words.set(id, NoteWords::parse(text));
}

pub(super) fn entries(words: &Words) -> Entries {
    words.snapshot().entries()
}

pub(super) fn expected(pairs: &[(&str, EntryId, u32)]) -> Entries {
    pairs
        .iter()
        .map(|&(word, id, count)| ((word.to_owned(), id), count))
        .collect()
}

pub(super) fn start(words: &mut Words) -> WordsMerge {
    let Some(merge) = words.start_merge() else {
        panic!("a merge starts");
    };
    merge
}

pub(super) fn hit(snapshot: &WordsSnapshot, term: &str, id: EntryId) -> Option<TermHit> {
    let Some(found) = snapshot.lookup(&NoteQuery::new(term, None, &[]), &AtomicBool::new(false))
    else {
        panic!("not cancelled");
    };
    found.get(0, id)
}

#[test]
fn set_replaces_a_note_s_words_and_remove_drops_them() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, a, "plan été plan");
    set(&mut words, b, "plan");
    assert_eq!(
        entries(&words),
        expected(&[("ete", a, 1), ("plan", a, 2), ("plan", b, 1)])
    );
    set(&mut words, a, "autre");
    let replaced = expected(&[("autre", a, 1), ("plan", b, 1)]);
    assert_eq!(entries(&words), replaced);
    words.merge_all();
    assert_eq!(entries(&words), replaced);

    set(&mut words, a, "neuf");
    assert_eq!(entries(&words), expected(&[("neuf", a, 1), ("plan", b, 1)]));
    words.remove(b);
    assert_eq!(entries(&words), expected(&[("neuf", a, 1)]));
    set(&mut words, a, "");
    assert_eq!(entries(&words), Entries::new(), "no words hides the note");
    words.merge_all();
    assert_eq!(entries(&words), Entries::new());
}

#[test]
fn an_emptied_note_keeps_its_ordinal_until_it_is_removed() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, a, "plan");
    set(&mut words, b, "autre");
    let span = words.snapshot().ordinal_span();
    for merged in [false, true, false] {
        set(&mut words, a, "");
        assert_eq!(entries(&words), expected(&[("autre", b, 1)]));
        if merged {
            words.merge_all();
        }
        set(&mut words, a, "neuf");
        assert_eq!(words.snapshot().ordinal_span(), span, "merged: {merged}");
        words.merge_all();
    }
    assert_eq!(
        entries(&words),
        expected(&[("autre", b, 1), ("neuf", a, 1)])
    );
    set(&mut words, a, "");
    words.remove(a);
    set(&mut words, a, "back");
    assert_eq!(words.snapshot().ordinal_span(), span + 1);
}

#[test]
fn remove_all_drops_every_gone_note() {
    let ids = ids(7);
    let mut words = Words::default();
    for &id in &ids[..3] {
        set(&mut words, id, "vieux");
    }
    words.merge_all();
    for &id in &ids[3..6] {
        set(&mut words, id, "neuf");
    }
    let gone: HashSet<EntryId> = [ids[0], ids[2], ids[3], ids[5], ids[6]].into();
    words.remove_all(&gone);
    let left = expected(&[("neuf", ids[4], 1), ("vieux", ids[1], 1)]);
    assert_eq!(entries(&words), left);
    words.merge_all();
    assert_eq!(entries(&words), left);
}

#[test]
fn a_held_words_snapshot_never_changes() {
    let ids = ids(4);
    let mut words = Words::default();
    set(&mut words, ids[0], "un");
    set(&mut words, ids[1], "deux");
    let mut held = vec![(words.snapshot(), entries(&words))];
    let running = start(&mut words);
    held.push((words.snapshot(), entries(&words)));
    set(&mut words, ids[0], "autre");
    set(&mut words, ids[2], "trois");
    held.push((words.snapshot(), entries(&words)));
    let _ = words.apply(running.run());
    held.push((words.snapshot(), entries(&words)));
    words.remove(ids[1]);
    held.push((words.snapshot(), entries(&words)));
    words.remove_all(&[ids[0], ids[2]].into());
    set(&mut words, ids[3], "quatre");
    words.merge_all();
    assert_eq!(entries(&words), expected(&[("quatre", ids[3], 1)]));
    for (snapshot, seen) in &held {
        assert_eq!(&snapshot.entries(), seen);
    }
}

#[test]
fn a_write_that_changes_nothing_copies_nothing() {
    let ids = ids(3);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    words.merge_all();
    let before = words.snapshot();
    set(&mut words, ids[1], "");
    set(&mut words, ids[1], "\u{301} !");
    words.remove(ids[2]);
    words.remove_all(&[ids[1], ids[2]].into());
    words.remove_all(&HashSet::new());
    let after = words.snapshot();
    assert!(before.ordinals.is_shared(&after.ordinals));
    assert!(Arc::ptr_eq(&before.segment, &after.segment));
    assert!(Arc::ptr_eq(&before.alive, &after.alive));
    assert!(before.fresh.is_shared(&after.fresh));
}

#[test]
fn a_new_note_under_a_snapshot_copies_one_shard_of_each_map() {
    let ids = ids(3 * SHARDS);
    let (settled, fresh) = ids.split_at(2 * SHARDS);
    let mut words = Words::default();
    for &id in settled {
        set(&mut words, id, "plan");
    }
    words.merge_all();
    for &id in &fresh[1..] {
        set(&mut words, id, "neuf");
    }
    let before = words.snapshot();
    set(&mut words, fresh[0], "neuf");
    let after = words.snapshot();
    assert_eq!(after.ordinals.shards_shared(&before.ordinals), SHARDS - 1);
    assert_eq!(after.fresh.shards_shared(&before.fresh), SHARDS - 1);
    assert_eq!(before.ordinals.len() + 1, after.ordinals.len());
    assert_eq!(hit(&before, "neuf", fresh[0]), None);
    assert!(hit(&after, "neuf", fresh[0]).is_some());
}

#[test]
fn a_write_under_a_snapshot_shares_the_segment() {
    let ids = ids(2);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    words.merge_all();
    let before = words.snapshot();
    set(&mut words, ids[0], "autre");
    words.remove(ids[1]);
    let after = words.snapshot();
    assert!(Arc::ptr_eq(&before.segment, &after.segment));
    assert!(!Arc::ptr_eq(&before.alive, &after.alive));
    assert_eq!(before.entries(), expected(&[("plan", ids[0], 1)]));
}

#[test]
fn a_removed_note_is_invisible_at_once() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, a, "plan");
    set(&mut words, b, "plan");
    words.merge_all();
    words.remove(a);
    let snapshot = words.snapshot();
    assert_eq!(snapshot.segment.notes(), 2, "not merged again");
    assert_eq!(snapshot.entries(), expected(&[("plan", b, 1)]));
    assert_eq!(hit(&snapshot, "pl", a), None);
    assert!(hit(&snapshot, "pl", b).is_some());
}

#[test]
fn re_setting_a_merged_note_shows_only_its_new_words() {
    let a = ids(1)[0];
    let mut words = Words::default();
    set(&mut words, a, "old shared");
    words.merge_all();
    set(&mut words, a, "new shared shared");
    let rewritten = expected(&[("new", a, 1), ("shared", a, 2)]);
    assert_eq!(entries(&words), rewritten);
    assert_eq!(
        hit(&words.snapshot(), "shared", a),
        Some(TermHit {
            count: 2,
            whole: true
        })
    );
    words.merge_all();
    assert_eq!(entries(&words), rewritten);
}

#[test]
fn a_merge_changes_no_entry() {
    let ids = ids(8);
    let mut words = Words::default();
    for (n, &id) in ids.iter().enumerate() {
        set(&mut words, id, &format!("plan note{n} common"));
    }
    words.merge_all();
    set(&mut words, ids[0], "plan rewritten");
    set(&mut words, ids[1], "");
    words.remove(ids[2]);
    set(&mut words, ids[3], "common common");
    let before = entries(&words);
    words.merge_all();
    assert_eq!(entries(&words), before);
    assert!(words.fresh.is_empty());
    assert_eq!(words.dead(), 0);
}

#[test]
fn many_notes_are_each_found_before_and_after_a_merge() {
    let ids = ids(300);
    let mut words = Words::default();
    for (n, &id) in ids.iter().enumerate() {
        set(&mut words, id, &format!("note{n} shared"));
    }
    for merged in [false, true] {
        if merged {
            words.merge_all();
        }
        let found = entries(&words);
        for (n, &id) in ids.iter().enumerate() {
            assert_eq!(found.get(&(format!("note{n}"), id)), Some(&1), "{n}");
        }
        assert_eq!(found.len(), 2 * ids.len());
    }
}

#[test]
fn heap_bytes_grows_with_the_words_and_returns_when_they_leave() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    let empty = words.snapshot().heap_bytes();

    set(&mut words, a, "plan");
    let one = words.snapshot().heap_bytes();
    assert!(one > empty);
    set(&mut words, b, "plan autre");
    assert!(words.snapshot().heap_bytes() > one);
    words.remove(b);
    assert_eq!(words.snapshot().heap_bytes(), one);

    words.merge_all();
    let merged = words.snapshot().heap_bytes();
    assert!(merged > empty);
    words.remove(a);
    assert!(
        words.snapshot().heap_bytes() > empty,
        "a removed note's postings stay until a merge"
    );
    words.merge_all();
    assert_eq!(words.snapshot().heap_bytes(), empty);
}

#[test]
fn heap_bytes_counts_the_segment_bits_fresh_notes_and_ordinals_exactly() {
    let ids = ids(3);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan été plan");
    set(&mut words, ids[1], "plan");
    words.merge_all();
    set(&mut words, ids[2], "autre autre");
    let snapshot = words.snapshot();
    let word = size_of::<usize>();

    // `ete` and `plan`, three `u32` offsets in each of `bounds` and `starts`, and one byte per
    // gap or count: `ete` (0, 1), `plan` (0, 2) then (1, 1).
    let segment = "eteplan".len() + 2 * 3 * size_of::<u32>() + 2 + 4;
    // Ordinals 0 and 1: one `u64` of bits.
    let bits = size_of::<u64>();
    // Its map slot (ordinal padded to a pointer, and the `Arc`), the `Arc`'s two counts and
    // `NoteWords`' three boxed slices, then `autre`, two `usize` bounds and one `u32` count.
    let fresh = (2 + 2 + 3 * 2) * word + "autre".len() + 2 * word + size_of::<u32>();
    // Three notes, each an `EntryId` and an `Ordinal`, both `u32`.
    let ordinals = 3 * (2 * size_of::<u32>());
    assert_eq!(snapshot.heap_bytes(), segment + bits + fresh + ordinals);
}
