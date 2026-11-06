use std::num::NonZeroUsize;

use proptest::prelude::*;
use proptest::sample::Index;

use super::{Fake, Harness, ops, run};
use crate::{Stop, Way};

impl Harness {
    /// `walk_to` from the note showing; the arrival and the place left, if any.
    fn walk_to(&mut self, stop: Stop<u8>) -> (Option<Fake>, Option<Fake>) {
        let showing = self.showing.note;
        let (arrived, left) =
            self.with_leaving(|tabs, leaving| tabs.walk_to(stop, showing, leaving));
        if let Some(place) = &arrived {
            self.showing = place.clone();
        }
        (arrived, left)
    }
}

fn way(back: bool) -> Way {
    if back { Way::Back } else { Way::Forward }
}

fn notes(places: &[Fake]) -> Vec<Option<u8>> {
    places.iter().map(|place| place.note).collect()
}

fn steps(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// Notes 1, 2, 3 behind, 4 showing.
fn four() -> Harness {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    for note in [1, 2, 3, 4] {
        harness.commit(tab, Some(note));
    }
    harness
}

proptest! {
    #[test]
    fn stops_list_where_successive_walks_arrive(ops in ops(), back in any::<bool>()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let stops: Vec<Stop<u8>> = harness.tabs.stops(way(back), harness.showing.note).collect();

        let mut arrivals = Vec::new();
        while let (Some(place), _) = harness.walk(back) {
            arrivals.push(place.note);
        }

        prop_assert_eq!(stops.iter().map(|stop| Some(stop.note)).collect::<Vec<_>>(), arrivals);
        prop_assert!(stops.iter().map(|stop| stop.steps.get()).eq(1..=stops.len()));
        prop_assert!(stops.iter().all(|stop| stop.way == way(back)));
    }

    #[test]
    fn a_walk_of_n_steps_is_n_walks_with_one_capture(
        ops in ops(),
        back in any::<bool>(),
        pick in any::<Index>(),
    ) {
        let mut one = run(&ops, |_, _, _, _, _| {});
        let mut many = run(&ops, |_, _, _, _, _| {});
        let stops: Vec<Stop<u8>> = one.tabs.stops(way(back), one.showing.note).collect();
        if stops.is_empty() {
            return Ok(());
        }
        let stop = stops[pick.index(stops.len())];
        let (left_one, left_many) = (one.left.len(), many.left.len());

        let (arrived, _) = one.walk_to(stop);
        let mut last = None;
        for _ in 0..stop.steps.get() {
            last = many.walk(back).0;
        }

        prop_assert!(arrived.is_some());
        prop_assert_eq!(&arrived, &last);
        let ([from_one, to_one], [from_many, to_many]) = (one.walked(back), many.walked(back));
        prop_assert_eq!(from_one, from_many);
        prop_assert_eq!(notes(&to_one), notes(&to_many));
        prop_assert_eq!(one.left.len() - left_one, 1, "one capture");
        prop_assert_eq!(many.left.len() - left_many, stop.steps.get());
        one.tabs.check();
    }

    #[test]
    fn a_listed_stop_arrives_on_its_note_or_walks_nothing(
        ops in ops(),
        at in any::<Index>(),
        pick in any::<Index>(),
        back in any::<bool>(),
    ) {
        let at = at.index(ops.len());
        let mut listed = Vec::new();
        let mut op = 0;
        let mut harness = run(&ops, |harness, _, _, _, _| {
            if op == at {
                listed = harness.tabs.stops(way(back), harness.showing.note).collect();
            }
            op += 1;
        });
        if listed.is_empty() {
            return Ok(());
        }
        let stop: Stop<u8> = listed[pick.index(listed.len())];
        let expect = harness
            .tabs
            .stops(stop.way, harness.showing.note)
            .nth(stop.steps.get() - 1)
            .is_some_and(|at| at.note == stop.note);
        let (stacks, left) = (harness.stacks(), harness.left.len());

        let (arrived, _) = harness.walk_to(stop);

        prop_assert_eq!(arrived.is_some(), expect);
        if let Some(place) = arrived {
            prop_assert_eq!(place.note, Some(stop.note));
        } else {
            prop_assert_eq!(harness.stacks(), stacks);
            prop_assert_eq!(harness.left.len(), left);
        }
        harness.tabs.check();
    }
}

#[test]
fn a_walk_of_n_steps_pushes_the_places_it_passes_as_stored() {
    let mut harness = four();
    let [back, _] = harness.walked(true);

    let (arrived, left) = harness.walk_to(Stop {
        way: Way::Back,
        steps: steps(3),
        note: 1,
    });

    assert_eq!(arrived, Some(back[0].clone()));
    let left = left.expect("an arrival leaves the place showing");
    assert_eq!(left.note, Some(4));
    assert_eq!(
        harness.walked(true),
        [Vec::new(), vec![left, back[2].clone(), back[1].clone()]]
    );
}

#[test]
fn a_walk_beyond_the_last_stop_changes_nothing() {
    let mut harness = four();
    let (stacks, left) = (harness.stacks(), harness.left.clone());

    let walked = harness.walk_to(Stop {
        way: Way::Back,
        steps: steps(4),
        note: 1,
    });

    assert_eq!(walked, (None, None));
    assert_eq!(harness.stacks(), stacks);
    assert_eq!(harness.left, left);
}

#[test]
fn a_stale_stop_walks_nothing() {
    let mut harness = Harness::new();
    let tab = harness.tabs.active();
    for note in [1, 2, 3] {
        harness.commit(tab, Some(note));
    }
    let stop = Stop {
        way: Way::Back,
        steps: steps(2),
        note: 1,
    };
    assert_eq!(harness.tabs.stops(Way::Back, Some(3)).nth(1), Some(stop));
    harness.commit(tab, Some(4));
    let (stacks, left) = (harness.stacks(), harness.left.clone());

    let walked = harness.walk_to(stop);

    assert_eq!(walked, (None, None));
    assert_eq!(harness.stacks(), stacks);
    assert_eq!(harness.left, left);
}
