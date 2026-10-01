use proptest::prelude::*;

use super::{Fake, Harness, Tabs, ops};
use crate::Tab;

fn places() -> impl Strategy<Value = (Vec<Fake>, usize)> {
    let place =
        (proptest::option::of(0..4_u8), 0..100_u32).prop_map(|(note, seq)| Fake { note, seq });
    proptest::collection::vec(place, 1..8).prop_flat_map(|places| {
        let len = places.len();
        (Just(places), 0..len)
    })
}

proptest! {
    #[test]
    fn restored_tabs_hold_the_places_in_order_but_the_active_one(
        (places, active) in places(),
    ) {
        let tabs = Tabs::<Fake>::restored(places.clone(), active);

        tabs.check();
        prop_assert_eq!(tabs.active_index(), active);
        let held: Vec<_> = tabs.iter().map(|tab| tab.place().cloned()).collect();
        let expected: Vec<_> = places
            .iter()
            .enumerate()
            .map(|(ix, place)| (ix != active).then(|| place.clone()))
            .collect();
        prop_assert_eq!(held, expected);
    }

    /// A close falls back through the MRU: the active tab, then the rest in bar order.
    #[test]
    fn the_mru_is_the_active_tab_then_bar_order((places, active) in places()) {
        let tabs = Tabs::<Fake>::restored(places, active);
        let mut expected = vec![tabs.active()];
        expected.extend(tabs.iter().map(Tab::id).filter(|&id| id != tabs.active()));
        prop_assert_eq!(&tabs.mru, &expected);
    }

    /// Ids stay unique, and each inactive tab gives back its place, under any later sequence.
    #[test]
    fn invariants_hold_over_any_sequence_after_a_restore((places, active) in places(), ops in ops()) {
        let showing = places[active].clone();
        let tabs = Tabs::restored(places, active);
        let mut harness = Harness { tabs, showing, ..Harness::new() };
        let mut shadow: std::collections::HashMap<_, _> = harness
            .tabs
            .iter()
            .filter_map(|tab| Some((tab.id(), tab.place()?.clone())))
            .collect();
        for op in &ops {
            harness.apply(op, &mut shadow);
            harness.tabs.check();
        }
    }
}

#[test]
fn a_restore_has_no_closed_tab() {
    let tabs = Tabs::<Fake>::restored(vec![Fake::default(), Fake::default()], 1);
    assert!(!tabs.has_closed());
}

#[test]
fn a_close_is_remembered() {
    let mut harness = Harness::new();
    harness.showing = Fake {
        note: Some(1),
        seq: 0,
    };
    let id = harness.tabs.active();
    harness.close(id);
    assert!(harness.tabs.has_closed());
}

#[test]
#[should_panic(expected = "out of range")]
fn a_restore_with_the_active_index_out_of_range_panics() {
    let _ = Tabs::<Fake>::restored(vec![Fake::default()], 1);
}

#[test]
#[should_panic(expected = "out of range")]
fn a_restore_of_no_place_panics() {
    let _ = Tabs::<Fake>::restored(Vec::new(), 0);
}
