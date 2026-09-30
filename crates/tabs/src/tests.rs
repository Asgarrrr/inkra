#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use proptest::sample::Index;

use super::{Closed, Direction, TabId, Tabs};

#[derive(Debug, Clone, Default, PartialEq)]
struct Fake {
    note: Option<u8>,
    seq: u32,
}

impl<P> Tabs<P> {
    /// M1–M3.
    fn check(&self) {
        assert!(!self.list.is_empty(), "at least one tab");
        let ids: Vec<TabId> = self.list.iter().map(|tab| tab.id).collect();
        let unique: HashSet<TabId> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "ids are unique");
        assert!(unique.contains(&self.active), "the active id is a tab");
        assert_eq!(
            self.mru.first(),
            Some(&self.active),
            "the active id leads the MRU"
        );
        let mru: HashSet<TabId> = self.mru.iter().copied().collect();
        assert_eq!(self.mru.len(), mru.len(), "the MRU holds each id once");
        assert_eq!(mru, unique, "the MRU is a permutation of the ids");
    }
}

/// Tabs of `Fake` whose `leaving` logs the tab left and returns its note with a new `seq`.
struct Harness {
    tabs: Tabs<Fake>,
    seq: u32,
    left: Vec<TabId>,
}

impl Harness {
    fn new() -> Self {
        Self {
            tabs: Tabs::new(Fake::default()),
            seq: 0,
            left: Vec::new(),
        }
    }

    fn fresh(&mut self, note: Option<u8>) -> Fake {
        self.seq += 1;
        Fake {
            note,
            seq: self.seq,
        }
    }

    /// Runs `op` with a logging `leaving`, and returns the place it gave, if it was called.
    fn with_leaving<R>(
        &mut self,
        op: impl FnOnce(&mut Tabs<Fake>, &mut dyn FnMut() -> Fake) -> R,
    ) -> (R, Option<Fake>) {
        let active = self.tabs.active();
        let note = self.tabs.active_place().note;
        let mut given = None;
        let mut seq = self.seq;
        let left = &mut self.left;
        let result = op(&mut self.tabs, &mut || {
            seq += 1;
            left.push(active);
            let place = Fake { note, seq };
            given = Some(place.clone());
            place
        });
        self.seq = seq;
        (result, given)
    }

    fn insert(&mut self, at: usize) -> TabId {
        let place = self.fresh(None);
        self.with_leaving(|tabs, leaving| tabs.insert(at, place, leaving))
            .0
    }

    fn activate(&mut self, id: TabId) -> Option<Fake> {
        self.with_leaving(|tabs, leaving| tabs.activate(id, leaving))
            .0
    }

    fn close(&mut self, id: TabId) -> Option<Closed<Fake>> {
        self.with_leaving(|tabs, leaving| tabs.close(id, leaving)).0
    }

    fn commit(&mut self, id: TabId, note: Option<u8>) -> bool {
        let place = self.fresh(note);
        self.with_leaving(|tabs, leaving| tabs.commit(id, place, leaving))
            .0
    }

    fn ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(super::Tab::id).collect()
    }

    fn places(&self) -> Vec<Fake> {
        self.tabs.iter().map(|tab| tab.place().clone()).collect()
    }
}

#[derive(Debug, Clone)]
enum Op {
    Insert(usize),
    Activate(Index),
    Step(Direction),
    Last,
    Close(Index),
    Commit(Index, Option<u8>),
    Forget(u8),
}

fn op() -> impl Strategy<Value = Op> {
    let direction = prop_oneof![Just(Direction::Next), Just(Direction::Previous)];
    prop_oneof![
        (0..12_usize).prop_map(Op::Insert),
        any::<Index>().prop_map(Op::Activate),
        direction.prop_map(Op::Step),
        Just(Op::Last),
        any::<Index>().prop_map(Op::Close),
        (any::<Index>(), proptest::option::of(0..4_u8)).prop_map(|(ix, note)| Op::Commit(ix, note)),
        (0..4_u8).prop_map(Op::Forget),
    ]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(op(), 1..64)
}

impl Harness {
    fn pick(&self, ix: Index) -> TabId {
        self.ids()[ix.index(self.tabs.count())]
    }

    /// Runs `op`, updates `shadow` (each tab's expected place), returns whether it left the active tab.
    fn apply(&mut self, op: &Op, shadow: &mut HashMap<TabId, Fake>) -> bool {
        let active = self.tabs.active();
        match op {
            Op::Insert(at) => {
                let place = self.fresh(None);
                let (id, left) =
                    self.with_leaving(|tabs, leaving| tabs.insert(*at, place.clone(), leaving));
                shadow.insert(active, left.expect("an insert leaves the active tab"));
                shadow.insert(id, place);
                true
            }
            Op::Activate(ix) => {
                let id = self.pick(*ix);
                self.activate_checked(id, shadow)
            }
            Op::Step(direction) => {
                let id = self.tabs.step(*direction);
                self.activate_checked(id, shadow)
            }
            Op::Last => {
                let id = self.tabs.last();
                self.activate_checked(id, shadow)
            }
            Op::Close(ix) => {
                let id = self.pick(*ix);
                let (closed, left) = self.with_leaving(|tabs, leaving| tabs.close(id, leaving));
                let closed = closed.expect("a tab closes");
                let stored = shadow.remove(&id).expect("the tab was shadowed");
                assert_eq!(
                    closed.place,
                    left.unwrap_or(stored),
                    "a close returns the tab's place"
                );
                if let Some((shown, place)) = &closed.shown {
                    let expected = shadow.entry(*shown).or_default();
                    assert_eq!(expected, place, "a close shows the stored place");
                }
                id == active
            }
            Op::Commit(ix, note) => {
                let id = self.pick(*ix);
                let place = self.fresh(*note);
                let committed = self
                    .with_leaving(|tabs, leaving| tabs.commit(id, place.clone(), leaving))
                    .0;
                assert_eq!(
                    committed,
                    id == active,
                    "a commit writes only the active tab"
                );
                if committed {
                    shadow.insert(id, place);
                }
                committed
            }
            Op::Forget(note) => {
                let matched = self.tabs.forget(|place| place.note == Some(*note));
                let active_matched = shadow[&active].note == Some(*note);
                assert_eq!(matched, active_matched, "forget reports the active tab");
                for place in shadow.values_mut() {
                    if place.note == Some(*note) {
                        *place = Fake::default();
                    }
                }
                false
            }
        }
    }

    fn activate_checked(&mut self, id: TabId, shadow: &mut HashMap<TabId, Fake>) -> bool {
        let active = self.tabs.active();
        let (arrived, left) = self.with_leaving(|tabs, leaving| tabs.activate(id, leaving));
        if id == active {
            assert_eq!(arrived, None, "activating the active tab returns nothing");
            return false;
        }
        shadow.insert(active, left.expect("an activation leaves the active tab"));
        assert_eq!(
            arrived.as_ref(),
            Some(&shadow[&id]),
            "an activation returns the stored place"
        );
        true
    }
}

/// Runs `ops`, checking M1–M3 after each; `after` gets the op, the prior active id, whether it was left, `shadow`.
fn run(
    ops: &[Op],
    mut after: impl FnMut(&Harness, &Op, TabId, bool, &HashMap<TabId, Fake>),
) -> Harness {
    let mut harness = Harness::new();
    let mut shadow = HashMap::from([(harness.tabs.active(), Fake::default())]);
    for op in ops {
        let active = harness.tabs.active();
        let leaves = harness.apply(op, &mut shadow);
        harness.tabs.check();
        after(&harness, op, active, leaves, &shadow);
    }
    harness
}

proptest! {
    #[test]
    fn invariants_hold_over_any_sequence(ops in ops()) {
        let mut seen: HashSet<TabId> = HashSet::new();
        let mut before: HashSet<TabId> = HashSet::new();
        let first = Harness::new().tabs.active();
        seen.insert(first);
        before.insert(first);
        run(&ops, |harness, _, _, _, _| {
            let now: HashSet<TabId> = harness.ids().into_iter().collect();
            for id in now.difference(&before) {
                assert!(seen.insert(*id), "an id is never reused");
            }
            before = now;
        });
    }

    #[test]
    fn an_insert_lands_at_its_index_clamped(ops in ops(), at in 0..12_usize) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        let before = harness.ids();
        let id = harness.insert(at);
        let mut expected = before.clone();
        expected.insert(at.min(before.len()), id);
        prop_assert_eq!(harness.ids(), expected);
        prop_assert_eq!(harness.tabs.active(), id);
        harness.tabs.check();
    }

    #[test]
    fn a_close_then_insert_at_its_index_restores_the_order(ops in ops(), ix in any::<Index>()) {
        let mut harness = run(&ops, |_, _, _, _, _| {});
        prop_assume!(harness.tabs.count() > 1);
        let before = harness.ids();
        let index = ix.index(before.len());
        let closed = harness.close(before[index]).expect("a tab closes");
        prop_assert_eq!(closed.index, index);
        let place = closed.place.clone();
        let id = harness.with_leaving(|tabs, leaving| tabs.insert(closed.index, place, leaving)).0;
        let expected: Vec<TabId> = before
            .iter()
            .map(|&other| if other == before[index] { id } else { other })
            .collect();
        prop_assert_eq!(harness.ids(), expected);
        prop_assert_eq!(harness.tabs.active_place(), &closed.place);
    }

    #[test]
    fn leaving_is_called_once_exactly_when_the_active_tab_is_left(ops in ops()) {
        let mut calls = 0;
        run(&ops, |harness, op, active, leaves, _| {
            assert_eq!(harness.left.len() - calls, usize::from(leaves), "{op:?}");
            if leaves {
                assert_eq!(harness.left.last(), Some(&active), "{op:?} left the active tab");
            }
            calls = harness.left.len();
        });
    }

    #[test]
    fn a_left_tab_gives_back_the_place_it_was_left_with(ops in ops()) {
        run(&ops, |harness, op, _, _, shadow| {
            let expected: Vec<Fake> = harness.ids().iter().map(|id| shadow[id].clone()).collect();
            assert_eq!(harness.places(), expected, "{op:?}");
        });
    }
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
    assert_eq!(harness.tabs.active_place(), &Fake::default());
    assert_eq!(closed.shown, Some((fresh, Fake::default())));
    assert_eq!(closed.place.note, Some(1));
    assert!(
        !harness.commit(only, Some(2)),
        "a load keyed by the old id lands nowhere"
    );
    assert_eq!(harness.tabs.active_place(), &Fake::default());
}

/// Tabs `[a, b, c]` in order, used `a`, `c`, `b` from least recent: `b` is active.
fn three() -> (Harness, [TabId; 3]) {
    let mut harness = Harness::new();
    let a = harness.tabs.active();
    let b = harness.insert(1);
    let c = harness.insert(2);
    harness.activate(b);
    (harness, [a, b, c])
}

#[test]
fn closing_the_active_tab_activates_the_most_recent_other() {
    let (mut harness, [a, b, c]) = three();
    harness.activate(a);
    let c_place = harness.tabs.iter().nth(2).unwrap().place().clone();
    let closed = harness.close(a).expect("a closes");
    assert_eq!(harness.tabs.active(), b, "b was used after c");
    assert_eq!(closed.shown, Some((b, harness.tabs.active_place().clone())));
    assert_eq!(harness.ids(), vec![b, c]);
    let closed = harness.close(b).expect("b closes");
    assert_eq!(closed.shown, Some((c, c_place)));
}

#[test]
fn closing_another_tab_keeps_the_active_one() {
    let (mut harness, [a, b, c]) = three();
    let place = harness.tabs.active_place().clone();
    let closed = harness.close(c).expect("c closes");
    assert_eq!(closed.shown, None);
    assert_eq!(closed.index, 2);
    assert_eq!(harness.tabs.active(), b);
    assert_eq!(harness.tabs.active_place(), &place);
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
fn forget_empties_only_matching_tabs() {
    let (mut harness, [a, b, c]) = three();
    harness.commit(b, Some(1));
    harness.activate(a);
    harness.commit(a, Some(2));
    harness.activate(c);
    harness.commit(c, Some(1));
    let a_place = harness.tabs.iter().next().unwrap().place().clone();

    assert!(
        harness.tabs.forget(|place| place.note == Some(1)),
        "c is active"
    );
    assert_eq!(
        harness.places(),
        vec![a_place.clone(), Fake::default(), Fake::default()]
    );
    assert_eq!(harness.tabs.active(), c);
    assert!(
        !harness.tabs.forget(|place| place.note == Some(2)),
        "a is not active"
    );
    assert_eq!(harness.places(), vec![Fake::default(); 3]);
}

#[test]
fn step_wraps_both_ways() {
    let (mut harness, [a, _, c]) = three();
    assert_eq!(harness.tabs.step(Direction::Next), c);
    assert_eq!(harness.tabs.step(Direction::Previous), a);
    harness.activate(c);
    assert_eq!(harness.tabs.step(Direction::Next), a);
    harness.activate(a);
    assert_eq!(harness.tabs.step(Direction::Previous), c);
    assert_eq!(harness.tabs.id_at(2), Some(c));
    assert_eq!(harness.tabs.id_at(3), None);
    assert_eq!(harness.tabs.last(), c);
}

#[test]
fn a_new_tab_goes_right_after_the_active_one() {
    let (mut harness, [a, b, c]) = three();
    assert_eq!(harness.tabs.active_index(), 1);
    let d = harness.insert(harness.tabs.active_index() + 1);
    assert_eq!(harness.ids(), vec![a, b, d, c]);
    assert_eq!(harness.tabs.active_index(), 2);
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
