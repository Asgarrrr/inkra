use proptest::prelude::*;
use proptest::sample::Index;

use super::closed::closed_entries;
use super::{Harness, ops, run, three};

proptest! {
    #[test]
    fn a_move_lands_at_its_index_clamped(ops in ops(), ix in any::<Index>(), to in 0..12_usize) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let id = harness.pick(ix);
        let mut expected = harness.ids();
        expected.remove(ix.index(expected.len()));
        expected.insert(to.min(expected.len()), id);

        harness.tabs.move_tab(id, to);

        prop_assert_eq!(harness.ids(), expected);
        harness.tabs.check();
    }

    #[test]
    fn a_move_changes_neither_the_mru_the_active_tab_nor_any_history(
        ops in ops(),
        ix in any::<Index>(),
        to in 0..12_usize,
    ) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let id = harness.pick(ix);
        let (mru, active) = (harness.tabs.mru.clone(), harness.tabs.active());
        let places = |harness: &Harness| {
            let mut places: Vec<_> = harness.ids().into_iter().zip(harness.places()).collect();
            places.sort_by_key(|(id, _)| id.0);
            places
        };
        let (before, stacks, closed) = (places(&harness), harness.stacks(), closed_entries(&harness));

        harness.tabs.move_tab(id, to);

        prop_assert_eq!(&harness.tabs.mru, &mru);
        prop_assert_eq!(harness.tabs.active(), active);
        prop_assert_eq!(places(&harness), before);
        prop_assert_eq!(harness.stacks(), stacks);
        prop_assert_eq!(closed_entries(&harness), closed);
    }
}

#[test]
fn moving_a_tab_onto_a_later_one_puts_it_after() {
    let (mut harness, [a, b, c]) = three();
    harness.tabs.move_tab(a, 2);
    assert_eq!(harness.ids(), [b, c, a]);
}

#[test]
fn moving_a_tab_onto_an_earlier_one_puts_it_before() {
    let (mut harness, [a, b, c]) = three();
    harness.tabs.move_tab(c, 0);
    assert_eq!(harness.ids(), [c, a, b]);
}

#[test]
fn moving_past_the_end_puts_it_last() {
    let (mut harness, [a, b, c]) = three();
    harness.tabs.move_tab(a, 7);
    assert_eq!(harness.ids(), [b, c, a]);
}

#[test]
fn moving_an_unknown_id_changes_nothing() {
    let (mut harness, [a, b, c]) = three();
    harness.close(c);
    harness.tabs.move_tab(c, 1);
    assert_eq!(harness.ids(), [a, b]);
}

#[test]
fn moving_the_active_tab_keeps_it_active() {
    let (mut harness, [a, b, c]) = three();
    harness.tabs.move_tab(b, 0);
    assert_eq!(harness.ids(), [b, a, c]);
    assert_eq!(harness.tabs.active(), b);
    assert_eq!(harness.tabs.active_index(), 0);
}
