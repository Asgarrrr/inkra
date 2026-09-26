use inkra_vault::EntryId;

use super::tests::{Entries, entries, expected, hit, ids, set, start};
use super::{MergedWords, Words, WordsMerge};
use crate::MergeWhen;
use crate::TermHit;
use crate::words::policy::{DEAD_PERCENT, FRESH_MIN, FRESH_RATIO};

#[test]
fn a_merge_starts_only_with_work_and_one_at_a_time() {
    let ids = ids(2);
    let mut words = Words::default();
    assert!(words.start_merge().is_none(), "nothing to merge");
    set(&mut words, ids[0], "plan");
    let running = start(&mut words);
    set(&mut words, ids[1], "plan");
    assert!(words.start_merge().is_none(), "one merge at a time");
    assert!(!words.due(MergeWhen::Due));
    let _ = words.apply(running.run());
    assert!(
        words.start_merge().is_some(),
        "the write during the merge is left"
    );
}

#[test]
fn a_delete_during_a_running_merge_stays_deleted_after_it_applies() {
    let ids = ids(3);
    let (merged, fresh, kept) = (ids[0], ids[1], ids[2]);
    let mut words = Words::default();
    set(&mut words, merged, "plan");
    words.merge_all();
    set(&mut words, fresh, "plan");
    set(&mut words, kept, "plan");
    let running = start(&mut words);
    words.remove(merged);
    words.remove_all(&[fresh].into());
    let _ = words.apply(running.run());
    let left = expected(&[("plan", kept, 1)]);
    assert_eq!(entries(&words), left);
    assert_eq!(hit(&words.snapshot(), "plan", merged), None);
    assert_eq!(hit(&words.snapshot(), "plan", fresh), None);
    words.merge_all();
    assert_eq!(entries(&words), left);
    assert_eq!(words.segment.notes(), 1);
}

#[test]
fn an_insert_during_a_running_merge_is_visible_once_after_it() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, a, "plan");
    let running = start(&mut words);
    set(&mut words, b, "plan plan");
    let _ = words.apply(running.run());
    let both = expected(&[("plan", a, 1), ("plan", b, 2)]);
    assert_eq!(entries(&words), both);
    assert_eq!(
        hit(&words.snapshot(), "plan", b),
        Some(TermHit {
            count: 2,
            whole: true
        })
    );
    assert_eq!(words.fresh.len(), 1, "b is still fresh");
    words.merge_all();
    assert_eq!(entries(&words), both);
}

#[test]
fn a_re_set_during_a_running_merge_keeps_only_its_new_words() {
    let ids = ids(2);
    let (merged, fresh) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, merged, "old one");
    words.merge_all();
    set(&mut words, fresh, "old two");
    let running = start(&mut words);
    set(&mut words, merged, "new one");
    set(&mut words, fresh, "new two");
    let _ = words.apply(running.run());
    let rewritten = expected(&[
        ("new", fresh, 1),
        ("new", merged, 1),
        ("one", merged, 1),
        ("two", fresh, 1),
    ]);
    assert_eq!(entries(&words), rewritten);
    assert_eq!(hit(&words.snapshot(), "old", merged), None);
    assert_eq!(hit(&words.snapshot(), "old", fresh), None);
    words.merge_all();
    assert_eq!(entries(&words), rewritten);
}

/// The result of another store's first merge, holding `id`.
fn foreign(id: EntryId) -> MergedWords {
    let mut words = Words::default();
    set(&mut words, id, "foreign");
    start(&mut words).run()
}

#[test]
fn a_stale_merge_is_dropped() {
    let ids = ids(3);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    let _ = words.apply(foreign(ids[2]));
    assert!(words.running.is_none());
    assert_eq!(
        entries(&words),
        expected(&[("plan", ids[0], 1)]),
        "no merge runs"
    );
    assert_eq!(words.fresh.len(), 1);

    words.merge_all();
    set(&mut words, ids[1], "autre");
    let running = start(&mut words);
    let _ = words.apply(foreign(ids[2]));
    assert!(
        words.running.is_some(),
        "another store's result leaves the running merge"
    );
    assert_eq!(words.segment.notes(), 1);
    let _ = words.apply(running.run());
    assert!(words.running.is_none());
    assert_eq!(words.segment.notes(), 2);
    assert_eq!(
        entries(&words),
        expected(&[("autre", ids[1], 1), ("plan", ids[0], 1)])
    );
}

#[test]
fn another_store_s_first_merge_is_dropped_during_this_store_s_first() {
    let ids = ids(3);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    let running = start(&mut words);
    let _ = words.apply(foreign(ids[2]));
    assert!(words.running.is_some(), "the running merge is kept");
    assert_eq!(words.segment.notes(), 0);
    assert_eq!(entries(&words), expected(&[("plan", ids[0], 1)]));
    let _ = words.apply(running.run());
    assert!(words.running.is_none());
    assert_eq!(words.segment.notes(), 1);
    assert_eq!(entries(&words), expected(&[("plan", ids[0], 1)]));
}

#[test]
fn a_dropped_merge_or_result_blocks_no_later_merge() {
    let ids = ids(4);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    drop(start(&mut words));
    set(&mut words, ids[1], "autre");
    drop(start(&mut words).run());
    set(&mut words, ids[2], "third");
    let running = start(&mut words);
    set(&mut words, ids[3], "late");
    assert!(words.start_merge().is_none(), "one live merge at a time");
    let _ = words.apply(running.run());
    assert_eq!(words.segment.notes(), 3);
    assert_eq!(
        words.fresh.len(),
        1,
        "the write during the merge stays fresh"
    );
    assert_eq!(
        entries(&words),
        expected(&[
            ("autre", ids[1], 1),
            ("late", ids[3], 1),
            ("plan", ids[0], 1),
            ("third", ids[2], 1)
        ])
    );
}

#[test]
fn a_dropped_merge_records_no_later_write() {
    let ids = ids(4);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    drop(start(&mut words));
    for &id in &ids[1..] {
        set(&mut words, id, "autre");
    }
    words.remove(ids[0]);
    assert!(words.running.is_none(), "the first write clears it");
}

#[test]
fn a_stale_apply_clears_a_dropped_merge() {
    let ids = ids(2);
    let mut words = Words::default();
    set(&mut words, ids[0], "plan");
    drop(start(&mut words).run());
    let _ = words.apply(foreign(ids[1]));
    assert!(words.running.is_none());
}

#[test]
fn a_merge_is_due_on_fresh_size_and_on_dead_share() {
    // Twice the live count past which the share, not FRESH_MIN, sets the bar.
    let live = 2 * FRESH_RATIO * FRESH_MIN;
    let ids = ids(live + live / (FRESH_RATIO - 1) + 2);
    let mut words = Words::default();
    assert!(!words.due(MergeWhen::Due), "an empty store");
    for &id in &ids[..FRESH_MIN - 1] {
        set(&mut words, id, "plan");
    }
    assert!(!words.due(MergeWhen::Due));
    set(&mut words, ids[FRESH_MIN - 1], "plan");
    assert!(words.due(MergeWhen::Due), "FRESH_MIN fresh notes");
    words.merge_all();
    assert!(!words.due(MergeWhen::Due));

    // Past FRESH_RATIO times FRESH_MIN live notes, the bar is a share of them.
    for &id in &ids[FRESH_MIN..live] {
        set(&mut words, id, "plan");
    }
    words.merge_all();
    let Some(bar) = (1..=live).find(|&f| f >= FRESH_MIN.max((live + f) / FRESH_RATIO)) else {
        panic!("some count of fresh notes is due");
    };
    assert!(bar > FRESH_MIN, "the share sets the bar");
    let mut fresh = ids[live..].iter();
    for &id in fresh.by_ref().take(bar - 1) {
        set(&mut words, id, "plan");
    }
    assert!(
        !words.due(MergeWhen::Due),
        "{} fresh of {}",
        bar - 1,
        live + bar - 1
    );
    for &id in fresh.take(1) {
        set(&mut words, id, "plan");
    }
    assert!(words.due(MergeWhen::Due));
    words.merge_all();

    // Dead notes: due at DEAD_PERCENT of the segment's.
    let notes = words.segment.notes();
    let due = (notes * DEAD_PERCENT).div_ceil(100);
    for &id in &ids[..due - 1] {
        words.remove(id);
    }
    assert!(!words.due(MergeWhen::Due), "{} dead of {notes}", due - 1);
    set(&mut words, ids[due - 1], "rewritten");
    assert!(
        words.due(MergeWhen::Due),
        "a re-set kills its merged postings too"
    );
    assert_eq!(words.fresh.len(), 1);
    let running = start(&mut words);
    assert!(!words.due(MergeWhen::Due), "not while a merge runs");
    drop(running);
    assert!(words.due(MergeWhen::Due), "a dropped merge runs no more");
    let running = start(&mut words);
    let _ = words.apply(running.run());
    assert!(!words.due(MergeWhen::Due));
}

#[test]
fn writes_never_merge_on_their_own() {
    let ids = ids(2 * FRESH_MIN);
    let mut words = Words::default();
    for &id in &ids {
        set(&mut words, id, "plan");
    }
    assert_eq!(words.fresh.len(), ids.len());
    words.merge_all();
    words.remove_all(&ids.iter().copied().collect());
    assert_eq!(words.dead(), ids.len());
    assert_eq!(entries(&words), Entries::new());
}

#[test]
fn ordinals_are_not_reused() {
    let ids = ids(2);
    let (a, b) = (ids[0], ids[1]);
    let mut words = Words::default();
    set(&mut words, a, "plan");
    let first = words.ordinals[&a];
    assert_eq!(words.snapshot().ordinal_span(), 1);
    set(&mut words, a, "autre");
    assert_eq!(words.ordinals[&a], first, "a re-set keeps its ordinal");
    words.merge_all();
    words.remove(a);
    set(&mut words, b, "plan");
    set(&mut words, a, "plan");
    let (second, third) = (words.ordinals[&b], words.ordinals[&a]);
    assert!(first < second && second < third);
    assert_eq!(
        words.snapshot().ordinal_span(),
        3,
        "three minted, the removed one never reused"
    );
    assert!(!words.alive.contains(first));
    assert_eq!(entries(&words), expected(&[("plan", a, 1), ("plan", b, 1)]));
}

#[test]
fn a_merge_and_its_result_cross_threads() {
    fn send<T: Send>() {}
    send::<WordsMerge>();
    send::<MergedWords>();
}
