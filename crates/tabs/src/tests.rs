use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use proptest::sample::Index;

use super::{Closed, Direction, HISTORY, Place, Stack, TabId, Tabs};
use harness::Harness;

mod closed;
mod harness;
mod history;
mod moving;
mod parked;
mod restored;

#[derive(Debug, Clone, Default, PartialEq)]
struct Fake {
    note: Option<u8>,
    seq: u32,
}

impl Place for Fake {
    type Note = u8;

    fn note(&self) -> Option<u8> {
        self.note
    }
}

/// Tells a place `resolve` gave apart from the one stored.
const RESOLVED: u32 = 1 << 20;

/// The harness's `resolve`: the stored place, marked.
fn resolved(mut place: Fake) -> Fake {
    place.seq += RESOLVED;
    place
}

fn entries(stack: &Stack<Fake>) -> Vec<Fake> {
    stack.0.iter().cloned().collect()
}

/// H1's adjacency, H2, H3.
fn check_stack<P: Place>(stack: &Stack<P>) {
    let stack = &stack.0;
    assert!(stack.len() <= HISTORY, "H3");
    assert!(stack.iter().all(|place| place.note().is_some()), "H2");
    assert!(
        stack
            .iter()
            .zip(stack.iter().skip(1))
            .all(|(older, newer)| older.note() != newer.note()),
        "H1: no adjacent entries on one note"
    );
}

impl<P: Place, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// M1–M3, H1's adjacency, H2, H3, H8, C1, C3.
    fn check(&self) {
        assert!(!self.list.is_empty(), "at least one tab");
        for tab in &self.list {
            assert_eq!(tab.place.is_none(), tab.id == self.active, "H8");
            check_stack(&tab.back);
            check_stack(&tab.forward);
        }
        assert!(self.closed.len() <= HISTORY, "C3");
        for entry in &self.closed {
            assert!(
                entry.place.note().is_some()
                    || !entry.back.0.is_empty()
                    || !entry.forward.0.is_empty(),
                "C1: no closed entry without a note or history"
            );
            check_stack(&entry.back);
            check_stack(&entry.forward);
        }
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

#[derive(Debug, Clone)]
enum Op {
    Insert(usize),
    Activate(Index),
    Step(Direction),
    Last,
    Close(Index),
    Commit(Index, Option<u8>),
    Forget(u8),
    Back,
    Forward,
    Reopen,
    Move(Index, usize),
}

/// Weighted toward commits and walks, so stacks grow deep enough to walk.
fn op() -> impl Strategy<Value = Op> {
    let direction = prop_oneof![Just(Direction::Next), Just(Direction::Previous)];
    prop_oneof![
        1 => (0..12_usize).prop_map(Op::Insert),
        1 => any::<Index>().prop_map(Op::Activate),
        1 => direction.prop_map(Op::Step),
        1 => Just(Op::Last),
        1 => any::<Index>().prop_map(Op::Close),
        4 => (any::<Index>(), proptest::option::of(0..4_u8)).prop_map(|(ix, note)| Op::Commit(ix, note)),
        1 => (0..4_u8).prop_map(Op::Forget),
        2 => Just(Op::Back),
        2 => Just(Op::Forward),
        2 => Just(Op::Reopen),
        1 => (any::<Index>(), 0..12_usize).prop_map(|(ix, to)| Op::Move(ix, to)),
    ]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(op(), 1..64)
}

impl Harness {
    fn pick(&self, ix: Index) -> TabId {
        self.ids()[ix.index(self.tabs.count())]
    }

    /// Runs `op`, updates `shadow` (each inactive tab's expected place), returns whether it left the active tab.
    fn apply(&mut self, op: &Op, shadow: &mut HashMap<TabId, Fake>) -> bool {
        let active = self.tabs.active();
        match op {
            Op::Insert(at) => {
                let (_, left) = self.with_leaving(|tabs, leaving| tabs.insert(*at, leaving));
                shadow.insert(active, left.expect("an insert leaves the active tab"));
                self.showing = self.fresh(None);
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
                let (closed, left) = self.with_leaving(|tabs, leaving| {
                    tabs.close(id, |slot| slot.map_or_else(leaving, resolved))
                });
                let closed = closed.expect("a tab closes");
                let expected = if id == active {
                    left.expect("closing the active tab leaves it")
                } else {
                    resolved(shadow.remove(&id).expect("an inactive tab is shadowed"))
                };
                assert_eq!(closed.place, expected, "a close returns the tab's place");
                if let Some((shown, place)) = &closed.shown {
                    let expected = shadow.remove(shown).unwrap_or_default();
                    assert_eq!(&expected, place, "a close shows the stored place");
                    self.showing = place.clone();
                }
                id == active
            }
            Op::Commit(ix, note) => {
                let id = self.pick(*ix);
                let committed = self.commit(id, *note);
                assert_eq!(
                    committed,
                    id == active,
                    "a commit writes only the active tab"
                );
                committed
            }
            Op::Back | Op::Forward => self.walk(matches!(op, Op::Back)).0.is_some(),
            Op::Reopen => self.reopen_checked(shadow),
            Op::Move(ix, to) => {
                let (id, mru) = (self.pick(*ix), self.tabs.mru.clone());
                let mut expected = self.ids();
                expected.retain(|&other| other != id);
                expected.insert((*to).min(expected.len()), id);
                self.tabs.move_tab(id, *to);
                assert_eq!(self.ids(), expected, "R2");
                assert_eq!((self.tabs.active(), &self.tabs.mru), (active, &mru), "R1");
                false
            }
            Op::Forget(note) => {
                self.forget(*note);
                for place in shadow.values_mut() {
                    if place.note == Some(*note) {
                        *place = Fake::default();
                    }
                }
                let active = self.tabs.iter().find(|tab| tab.id() == active);
                assert_eq!(
                    active.and_then(|tab| tab.place()),
                    None,
                    "forget leaves the active tab"
                );
                false
            }
        }
    }

    /// C2, C4 and C5 against the newest closed entry, snapshot before the reopen.
    fn reopen_checked(&mut self, shadow: &mut HashMap<TabId, Fake>) -> bool {
        let (active, ids, closed) = (self.tabs.active(), self.ids(), self.tabs.closed.len());
        let entry = self.tabs.closed.back().cloned();
        let (arrived, left) = self.with_leaving(|tabs, leaving| tabs.reopen(leaving));
        let Some(entry) = entry else {
            assert_eq!(
                (arrived, left),
                (None, None),
                "C5: nothing closed, no closure"
            );
            assert_eq!((self.tabs.active(), self.ids()), (active, ids), "C5");
            return false;
        };
        let id = self.tabs.active();
        assert!(!ids.contains(&id), "C4: a fresh id");
        let mut expected = ids.clone();
        expected.insert(entry.index.min(ids.len()), id);
        assert_eq!(self.ids(), expected, "C2");
        assert_eq!(
            self.tabs.closed.len(),
            closed - 1,
            "C5: the entry is popped"
        );
        assert_eq!(
            arrived.as_ref(),
            Some(&entry.place),
            "C4: the entry's place"
        );
        let tab = self.tabs.iter().find(|tab| tab.id == id).expect("a tab");
        assert_eq!(
            [entries(&tab.back), entries(&tab.forward)],
            [entries(&entry.back), entries(&entry.forward)],
            "C4: the entry's history"
        );
        shadow.insert(active, left.expect("a reopen leaves the active tab"));
        self.showing = entry.place;
        true
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
            arrived,
            shadow.remove(&id),
            "an activation returns the stored place"
        );
        self.showing = arrived.expect("an inactive tab holds a place");
        true
    }
}

/// Runs `ops`, checking M1–M3 after each; `after` gets the op, the prior active id, whether it was left, `shadow`.
fn run(
    ops: &[Op],
    mut after: impl FnMut(&Harness, &Op, TabId, bool, &HashMap<TabId, Fake>),
) -> Harness {
    let mut harness = Harness::new();
    let mut shadow = HashMap::new();
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
        let id = harness.with_leaving(|tabs, leaving| tabs.insert(closed.index, leaving)).0;
        let expected: Vec<TabId> = before
            .iter()
            .map(|&other| if other == before[index] { id } else { other })
            .collect();
        prop_assert_eq!(harness.ids(), expected);
        harness.tabs.check();
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
            let expected: Vec<Option<Fake>> = harness.ids().iter().map(|id| shadow.get(id).cloned()).collect();
            assert_eq!(harness.places(), expected, "{op:?}");
        });
    }
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
