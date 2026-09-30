use proptest::prelude::*;
use proptest::sample::Index;

use super::{Fake, Harness, Op, TabId, entries, op, ops, resolved, run, three};
use crate::HISTORY;

/// Weighted toward closes and reopens, so a reopen often finds an entry and a bar shorter than its index.
fn closing_ops() -> impl Strategy<Value = Vec<Op>> {
    let closing = prop_oneof![
        3 => any::<Index>().prop_map(Op::Close),
        3 => Just(Op::Reopen),
        2 => op(),
    ];
    proptest::collection::vec(closing, 1..64)
}

/// Each closed entry's place and stacks, oldest first.
pub(super) fn closed_entries(harness: &Harness) -> Vec<(Fake, [Vec<Fake>; 2])> {
    harness
        .tabs
        .closed
        .iter()
        .map(|entry| {
            (
                entry.place.clone(),
                [entries(&entry.back), entries(&entry.forward)],
            )
        })
        .collect()
}

proptest! {
    /// `reopen_checked` asserts C2 on every reopen.
    #[test]
    fn a_reopen_lands_at_its_old_index_clamped(ops in closing_ops()) {
        run(&ops, |_, _, _, _, _| {});
    }

    #[test]
    fn a_close_then_reopen_restores_the_order_place_and_history(ops in ops(), ix in any::<Index>()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        // Closing the last tab appends a fresh one, so the order claim does not hold there.
        prop_assume!(harness.tabs.count() > 1);
        let before = harness.ids();
        let id = before[ix.index(before.len())];
        let stacks = harness.stacks()[&id].clone();
        let entries = harness.tabs.closed.len();

        let closed = harness.close(id).expect("a tab closes");
        prop_assume!(harness.tabs.closed.len() == entries + 1);
        let place = harness.reopen();

        let new = harness.tabs.active();
        let expected: Vec<TabId> = before.iter().map(|&other| if other == id { new } else { other }).collect();
        prop_assert_eq!(harness.ids(), expected);
        prop_assert_eq!(place, Some(closed.place));
        prop_assert_eq!(&harness.stacks()[&new], &stacks);
        harness.tabs.check();
    }

    #[test]
    fn a_closed_tab_is_kept_iff_it_has_a_note_or_history(ops in ops(), ix in any::<Index>()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let id = harness.pick(ix);
        let history = harness.stacks()[&id].iter().any(|stack| !stack.is_empty());
        let entries = harness.tabs.closed.len();

        let closed = harness.close(id).expect("a tab closes");

        let kept = closed.place.note.is_some() || history;
        let expected = if kept { (entries + 1).min(HISTORY) } else { entries };
        prop_assert_eq!(harness.tabs.closed.len(), expected);
        harness.tabs.check();
    }

    #[test]
    fn forget_drops_a_note_from_every_closed_tab_keeping_the_rest_in_order(ops in closing_ops(), note in 0..4_u8) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let before = closed_entries(&harness);

        harness.forget(note);

        let forgotten = |stack: Vec<Fake>| {
            let mut kept: Vec<Fake> = stack.into_iter().rev().filter(|place| place.note != Some(note)).collect();
            kept.dedup_by_key(|place| place.note);
            kept.reverse();
            kept
        };
        let expected: Vec<(Fake, [Vec<Fake>; 2])> = before
            .into_iter()
            .map(|(place, stacks)| {
                let place = if place.note == Some(note) { Fake::default() } else { place };
                (place, stacks.map(forgotten))
            })
            .filter(|(place, stacks)| place.note.is_some() || stacks.iter().any(|stack| !stack.is_empty()))
            .collect();
        prop_assert_eq!(closed_entries(&harness), expected);
        harness.tabs.check();
    }

    #[test]
    fn reopen_pops_the_newest_closed_tab(ops in closing_ops()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let closed: Vec<Fake> = harness.tabs.closed.iter().map(|entry| entry.place.clone()).collect();

        let reopened: Vec<Fake> = std::iter::from_fn(|| harness.reopen()).take(closed.len() + 1).collect();

        prop_assert_eq!(reopened, closed.into_iter().rev().collect::<Vec<_>>());
        harness.tabs.check();
    }
}

#[test]
fn reopen_with_nothing_closed_calls_no_leaving() {
    let mut harness = Harness::new();
    let ids = harness.ids();

    assert_eq!(harness.reopen(), None);

    assert!(harness.left.is_empty());
    assert_eq!(harness.ids(), ids);
}

#[test]
fn closing_an_empty_tab_with_no_history_keeps_nothing() {
    let mut harness = Harness::new();
    let empty = harness.insert(1);

    harness.close(empty);
    let left = harness.left.len();

    assert_eq!(harness.reopen(), None);
    assert_eq!(harness.left.len(), left, "no `leaving`");
}

#[test]
fn closing_an_inactive_tab_keeps_the_place_resolve_gives() {
    let (mut harness, [a, b, _]) = three();
    harness.activate(a);
    harness.commit(a, Some(1));
    harness.activate(b);
    let stored = harness.places()[0].clone().unwrap();

    let closed = harness.close(a).unwrap();

    assert_eq!(closed.place, resolved(stored.clone()));
    assert_eq!(harness.reopen(), Some(resolved(stored)));
}

#[test]
fn closing_the_active_tab_calls_no_resolve() {
    let (mut harness, [_, b, _]) = three();
    let mut resolves = 0;

    let (closed, left) = harness.with_leaving(|tabs, leaving| {
        tabs.close(b, leaving, |place| {
            resolves += 1;
            place
        })
    });

    assert_eq!(resolves, 0);
    assert_eq!(closed.map(|closed| closed.place), left);
}

#[test]
fn a_reopened_tab_gets_a_fresh_id() {
    let (mut harness, [a, _, _]) = three();
    harness.activate(a);
    harness.commit(a, Some(1));
    harness.close(a);

    assert!(harness.reopen().is_some());

    let reopened = harness.tabs.active();
    assert_ne!(reopened, a);
    assert!(
        !harness.commit(a, Some(1)),
        "a load keyed by the old id lands nowhere"
    );
    assert_eq!(harness.activate(a), None);
    assert_eq!(harness.tabs.active(), reopened);
}

#[test]
fn reopening_after_closing_the_last_tab_puts_it_before_the_empty_one() {
    let mut harness = Harness::new();
    let a = harness.tabs.active();
    harness.commit(a, Some(1));
    harness.close(a);
    let fresh = harness.tabs.active();

    let place = harness.reopen();

    let reopened = harness.tabs.active();
    assert_eq!(harness.ids(), [reopened, fresh]);
    assert_eq!(place.map(|place| place.note), Some(Some(1)));
    harness.tabs.check();
}

#[test]
fn the_closed_stack_keeps_its_newest_1024() {
    let mut harness = Harness::new();
    let closed: Vec<Fake> = (0..=HISTORY)
        .map(|n| {
            let id = harness.insert(1);
            harness.commit(id, Some(u8::try_from(n % 4).unwrap()));
            harness.close(id).unwrap().place
        })
        .collect();
    assert_eq!(harness.tabs.closed.len(), HISTORY);

    let reopened: Vec<Fake> = std::iter::from_fn(|| harness.reopen())
        .take(HISTORY + 1)
        .collect();

    let newest: Vec<Fake> = closed[1..].iter().rev().cloned().collect();
    assert_eq!(reopened, newest, "the oldest close dropped");
}

#[test]
fn forget_drops_a_closed_tab_left_with_nothing() {
    let mut harness = Harness::new();
    let id = harness.insert(1);
    harness.commit(id, Some(1));
    harness.close(id);

    harness.forget(1);

    assert_eq!(harness.reopen(), None);
}

#[test]
fn forget_empties_a_closed_tab_with_history() {
    let mut harness = Harness::new();
    let id = harness.insert(1);
    harness.commit(id, Some(1));
    harness.commit(id, Some(2));
    harness.close(id);

    harness.forget(2);

    assert_eq!(harness.reopen().map(|place| place.note), Some(None));
    let (back, _) = harness.walk(true);
    assert_eq!(back.map(|place| place.note), Some(Some(1)));
}

#[test]
fn closing_the_last_tab_leaves_one_empty_tab_with_a_new_id() {
    let mut harness = Harness::new();
    let only = harness.tabs.active();
    harness.commit(only, Some(1));
    let closed = harness.close(only).expect("the tab closes");
    assert_eq!(harness.tabs.count(), 1);
    let fresh = harness.tabs.active();
    assert_ne!(fresh, only);
    assert_eq!(harness.places(), vec![None]);
    assert_eq!(closed.shown, Some((fresh, Fake::default())));
    assert_eq!(closed.place.note, Some(1));
    assert!(
        !harness.commit(only, Some(2)),
        "a load keyed by the old id lands nowhere"
    );
    assert_eq!(harness.showing, Fake::default());
}

#[test]
fn closing_the_active_tab_activates_the_most_recent_other() {
    let (mut harness, [a, b, c]) = three();
    harness.activate(a);
    let [b_place, c_place] = [1, 2].map(|ix| harness.places()[ix].clone().unwrap());
    let closed = harness.close(a).expect("a closes");
    assert_eq!(harness.tabs.active(), b, "b was used after c");
    assert_eq!(closed.shown, Some((b, b_place)));
    assert_eq!(harness.ids(), vec![b, c]);
    let closed = harness.close(b).expect("b closes");
    assert_eq!(closed.shown, Some((c, c_place)));
}

#[test]
fn closing_another_tab_keeps_the_active_one() {
    let (mut harness, [a, b, c]) = three();
    let closed = harness.close(c).expect("c closes");
    assert_eq!(closed.shown, None);
    assert_eq!(closed.index, 2);
    assert_eq!(harness.tabs.active(), b);
    assert_eq!(harness.places()[1], None);
    harness.close(b);
    assert_eq!(harness.tabs.active(), a, "c left the MRU");
}

#[test]
fn a_commit_to_a_tab_not_active_changes_nothing() {
    let (mut harness, [a, _, _]) = three();
    let places = harness.places();
    assert!(!harness.commit(a, Some(3)));
    assert_eq!(harness.places(), places);
    assert_eq!(harness.activate(a).map(|place| place.note), Some(None));
}

#[test]
fn closing_an_inactive_tab_does_not_call_leaving() {
    let (mut harness, [a, _, c]) = three();
    let calls = harness.left.len();
    harness.close(a);
    harness.close(c);
    assert_eq!(harness.left.len(), calls);
}

#[test]
fn a_commit_to_an_inactive_tab_does_not_call_leaving() {
    let (mut harness, [a, _, _]) = three();
    let calls = harness.left.len();
    harness.commit(a, Some(1));
    assert_eq!(harness.left.len(), calls);
}
