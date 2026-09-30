use std::collections::HashMap;

use proptest::prelude::*;

use super::{Fake, Harness, Op, TabId, ops, run};
use crate::Stack;

fn entries(stack: &Stack<Fake>) -> Vec<Fake> {
    stack.0.iter().cloned().collect()
}

fn fake(note: u8, seq: usize) -> Fake {
    Fake {
        note: Some(note),
        seq: u32::try_from(seq).unwrap(),
    }
}

impl Harness {
    /// The active tab's stacks, the one walked from first.
    fn walked(&self, back: bool) -> [Vec<Fake>; 2] {
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.id == self.tabs.active())
            .unwrap();
        let [from, to] = if back {
            [&tab.back, &tab.forward]
        } else {
            [&tab.forward, &tab.back]
        };
        [entries(from), entries(to)]
    }

    fn stacks(&self) -> HashMap<TabId, [Vec<Fake>; 2]> {
        self.tabs
            .iter()
            .map(|tab| (tab.id, [entries(&tab.back), entries(&tab.forward)]))
            .collect()
    }
}

/// H1, H4: the arrival is the newest entry off the note showing, and the place left tops the other stack.
fn walk_checked(harness: &mut Harness, back: bool) {
    let showing = harness.showing.note;
    let [from, to] = harness.walked(back);
    let newest = from.iter().rposition(|place| place.note != showing);

    let (arrived, left) = harness.walk(back);

    assert_eq!(arrived, newest.map(|at| from[at].clone()));
    let [from_after, to_after] = harness.walked(back);
    assert_eq!(
        from_after,
        from[..newest.unwrap_or(0)],
        "the entries skipped are dropped"
    );
    match (arrived, left) {
        (None, left) => {
            assert_eq!(left, None, "nothing arrived, nothing left");
            assert_eq!(to_after, to);
        }
        (Some(_), Some(left)) if left.note.is_some() => {
            assert_eq!(
                to_after.last(),
                Some(&left),
                "the place left tops the other stack"
            );
        }
        (Some(_), Some(_)) => assert_eq!(to_after, to, "an empty place left is not pushed"),
        (Some(_), None) => panic!("an arrival leaves the place showing"),
    }
    harness.tabs.check();
}

fn history_ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(super::op(), 1..256)
}

proptest! {
    #[test]
    fn history_invariants_hold_over_any_sequence(ops in history_ops()) {
        run(&ops, |_, _, _, _, _| {});
    }

    #[test]
    fn back_lands_on_the_newest_entry_on_another_note(ops in ops()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        walk_checked(&mut harness, true);
    }

    #[test]
    fn forward_mirrors_back(ops in ops()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        walk_checked(&mut harness, false);
    }

    #[test]
    fn back_then_forward_returns_to_the_note_left(ops in ops()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        if let (Some(_), Some(left)) = harness.walk(true)
            && left.note.is_some()
        {
            prop_assert_eq!(harness.walk(false).0, Some(left));
        }
    }

    #[test]
    fn a_navigation_pushes_the_place_left_and_clears_forward(ops in ops(), note in proptest::option::of(0..4_u8)) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let [back, _] = harness.walked(true);
        let active = harness.tabs.active();

        let (committed, left) = harness.with_leaving(|tabs, leaving| tabs.commit(active, leaving));
        prop_assert!(committed);
        harness.showing = harness.fresh(note);

        let left = left.unwrap();
        let [back_after, forward_after] = harness.walked(true);
        prop_assert_eq!(forward_after, Vec::<Fake>::new(), "a navigation clears forward");
        if left.note.is_some() {
            prop_assert_eq!(back_after.last(), Some(&left));
        } else {
            prop_assert_eq!(back_after, back, "an empty place left is not pushed");
        }
        harness.tabs.check();
    }

    #[test]
    fn a_tab_switch_changes_no_history(ops in ops()) {
        let mut harness = Harness::new();
        let mut shadow = HashMap::new();
        for op in &ops {
            let before = harness.stacks();
            harness.apply(op, &mut shadow);
            if matches!(op, Op::Activate(_) | Op::Step(_) | Op::Last | Op::Insert(_) | Op::Close(_)) {
                for (id, stacks) in harness.stacks() {
                    let expected = before.get(&id).cloned().unwrap_or_default();
                    prop_assert_eq!(stacks, expected, "{:?}", op);
                }
            }
        }
    }

    #[test]
    fn forget_drops_a_note_from_every_stack_keeping_the_rest_in_order(ops in ops(), note in 0..4_u8) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let before = harness.stacks();

        harness.forget(note);

        for (id, stacks) in harness.stacks() {
            let expected = before[&id].clone().map(|stack| {
                let mut kept: Vec<Fake> = stack.into_iter().rev().filter(|place| place.note != Some(note)).collect();
                kept.dedup_by_key(|place| place.note);
                kept.reverse();
                kept
            });
            prop_assert_eq!(stacks, expected);
        }
        harness.tabs.check();
    }

    #[test]
    fn a_stack_keeps_its_newest_1024_entries_with_no_adjacent_repeat(
        notes in proptest::collection::vec(0..3_u8, 0..3000),
    ) {
        let mut stack = Stack::default();
        let mut all: Vec<Fake> = Vec::new();
        for (seq, note) in notes.into_iter().enumerate() {
            let place = fake(note, seq);
            stack.push(place.clone());
            if all.last().is_some_and(|top| top.note == place.note) {
                all.pop();
            }
            all.push(place);
        }
        prop_assert_eq!(entries(&stack), all[all.len().saturating_sub(1024)..].to_vec());
    }
}

#[test]
fn back_skips_entries_on_the_note_showing() {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    for note in [2, 1, 1] {
        harness.commit(tab, Some(note));
    }
    assert_eq!(harness.walked(true)[0].len(), 2, "back holds 2 then 1");

    let (arrived, _) = harness.walk(true);

    assert_eq!(arrived.map(|place| place.note), Some(Some(2)));
    let [back, forward] = harness.walked(true);
    assert!(back.is_empty(), "the entry on the note showing is dropped");
    assert_eq!(
        forward.iter().map(|place| place.note).collect::<Vec<_>>(),
        [Some(1)]
    );
}

#[test]
fn back_with_nothing_behind_calls_no_leaving() {
    let mut harness = Harness::new();
    assert_eq!(harness.walk(true), (None, None));
    let tab = harness.tabs.active();
    harness.commit(tab, Some(1));
    harness.commit(tab, Some(1));
    assert_eq!(harness.walked(true)[0].len(), 1);

    assert_eq!(
        harness.walk(true),
        (None, None),
        "only the note showing is behind"
    );
    assert_eq!(harness.walked(true), [Vec::<Fake>::new(), Vec::new()]);
}

#[test]
fn an_empty_place_left_is_not_pushed() {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    harness.commit(tab, Some(1));
    assert!(harness.walked(true)[0].is_empty(), "the empty first place");
    harness.commit(tab, Some(2));
    harness.forget(2);

    harness.commit(tab, Some(3));

    let back = harness.walked(true)[0].clone();
    assert_eq!(
        back.iter().map(|place| place.note).collect::<Vec<_>>(),
        [Some(1)]
    );
}

#[test]
fn forget_collapses_the_entries_it_separated_keeping_the_newest() {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    for note in [1, 2, 1, 3] {
        harness.commit(tab, Some(note));
    }
    let back = harness.walked(true)[0].clone();
    assert_eq!(
        back.iter().map(|place| place.note).collect::<Vec<_>>(),
        [Some(1), Some(2), Some(1)]
    );

    harness.forget(2);

    assert_eq!(harness.walked(true)[0], [back[2].clone()]);
}

#[test]
fn a_full_stack_drops_its_oldest() {
    let mut stack = Stack::default();
    for seq in 0..1025 {
        stack.push(fake(u8::from(seq % 2 == 1), seq));
    }
    assert_eq!(stack.0.len(), 1024);
    assert_eq!(stack.0.front(), Some(&fake(1, 1)));
}

#[test]
fn a_stack_of_1024_keeps_its_oldest() {
    let mut stack = Stack::default();
    for seq in 0..1024 {
        stack.push(fake(u8::from(seq % 2 == 1), seq));
    }
    assert_eq!(stack.0.len(), 1024);
    assert_eq!(stack.0.front(), Some(&fake(0, 0)));
}

#[test]
fn a_push_onto_the_same_note_replaces_the_top() {
    let mut stack = Stack::default();
    stack.push(fake(2, 0));
    stack.push(fake(1, 1));
    stack.push(fake(1, 2));
    assert_eq!(entries(&stack), [fake(2, 0), fake(1, 2)]);
}

#[test]
fn a_new_tab_starts_with_no_history() {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    for note in [1, 2, 3] {
        harness.commit(tab, Some(note));
    }
    harness.walk(true);

    harness.insert(1);

    assert_eq!(harness.walked(true), [Vec::<Fake>::new(), Vec::new()]);
    assert_eq!(harness.walk(true), (None, None));
}
