//! A tab moved between groups with its history (splits spec, P4).

use proptest::prelude::*;
use proptest::sample::Index;

use super::{Fake, Harness, TabId, Tabs, ops, run, three};
use crate::Detached;

impl Harness {
    /// Shows the place the take returns, as the app does.
    fn take(&mut self, id: TabId) -> Option<Detached<Fake>> {
        let (detached, shown) = self
            .with_leaving(|tabs, leaving| tabs.take(id, leaving))
            .0?;
        if let Some((_, place)) = shown {
            self.showing = place;
        }
        Some(detached)
    }

    fn put(&mut self, detached: Detached<Fake>, at: usize) -> TabId {
        let (id, place) = self
            .with_leaving(|tabs, leaving| tabs.put(detached, at, leaving))
            .0;
        self.showing = place;
        id
    }

    /// Each tab's note, the active one's from the screen, sorted.
    fn notes(&self) -> Vec<Option<u8>> {
        let mut notes: Vec<_> = self
            .tabs
            .iter()
            .map(|tab| tab.place().map_or(self.showing.note, |place| place.note))
            .collect();
        notes.sort_unstable();
        notes
    }
}

/// One tab whose history reads back `[1]`, forward `[3]`, on note 2.
fn walked() -> Harness {
    let mut harness = Harness::new();
    let id = harness.tabs.active();
    for note in 1..=3 {
        harness.commit(id, Some(note));
    }
    harness.walk(true);
    harness
}

#[test]
fn take_an_inactive_tab() {
    let (mut harness, [a, b, c]) = three();
    let slot = harness.places()[0].clone().expect("a is inactive");
    let (showing, left) = (harness.showing.clone(), harness.left.len());

    let detached = harness.take(a).expect("a is a tab");

    assert_eq!(harness.ids(), [b, c]);
    assert_eq!(harness.tabs.active(), b);
    assert_eq!(harness.tabs.mru, [b, c]);
    assert_eq!(
        (&harness.showing, harness.left.len()),
        (&showing, left),
        "nothing is left"
    );
    assert!(!harness.tabs.has_closed(), "a take closes nothing");
    assert!(harness.take(a).is_none(), "a is gone");
    harness.tabs.check();
    let mut other = Harness::new();
    other.put(detached, 0);
    assert_eq!(other.showing, slot, "the slot moves with the tab");
}

#[test]
fn take_the_active_tab_shows_the_next() {
    let (mut harness, [a, b, c]) = three();
    let next = harness.places()[2].clone().expect("c is inactive");

    let ((detached, shown), given) =
        harness.with_leaving(|tabs, leaving| tabs.take(b, leaving).expect("b is a tab"));

    assert_eq!(harness.left.last(), Some(&b), "the take leaves b");
    assert_eq!(shown, Some((c, next)), "the MRU's next shows");
    assert_eq!(harness.ids(), [a, c]);
    assert_eq!(harness.tabs.active(), c);
    assert_eq!(harness.tabs.mru, [c, a]);
    assert!(!harness.tabs.has_closed(), "a take closes nothing");
    let mut other = Harness::new();
    other.put(detached, 0);
    assert_eq!(
        Some(other.showing),
        given,
        "the place left moves with the tab"
    );
}

#[test]
#[should_panic(expected = "the only tab")]
fn take_panics_on_a_sole_tab() {
    let mut harness = Harness::new();
    let id = harness.tabs.active();
    harness.take(id);
}

#[test]
fn put_activates_and_parks_the_old() {
    let (mut source, [a, _, c]) = three();
    let slot = source.places()[0].clone().expect("a is inactive");
    let mut harness = Harness::new();
    let x = harness.tabs.active();
    let y = harness.insert(1);
    let showing = harness.showing.clone();
    let detached = source.take(a).expect("a is a tab");

    let id = harness.put(detached, 1);

    assert!(![x, y].contains(&id), "a fresh id");
    assert_eq!(harness.ids(), [x, id, y]);
    assert_eq!(harness.tabs.active(), id);
    assert_eq!(harness.tabs.mru, [id, y, x]);
    assert_eq!(harness.left.last(), Some(&y), "the put parks y");
    assert_eq!(
        harness.places()[2].as_ref().map(|place| place.note),
        Some(showing.note)
    );
    assert_eq!(harness.showing, slot);
    harness.tabs.check();

    let detached = source.take(c).expect("c is a tab");
    let last = harness.put(detached, 9);
    assert_eq!(harness.ids(), [x, id, y, last], "clamped to the end");
    harness.tabs.check();
}

#[test]
fn a_put_tab_keeps_its_history() {
    let mut source = walked();
    let id = source.tabs.active();
    let stacks = source.stacks()[&id].clone();
    source.insert(1);
    let detached = source.take(id).expect("a tab");
    let mut harness = Harness::new();

    let put = harness.put(detached, 1);

    assert_eq!(harness.stacks()[&put], stacks);
    assert_eq!(harness.showing.note, Some(2));
    harness.tabs.check();
}

#[test]
fn into_detached_and_detached_round_trip() {
    let harness = walked();
    let stacks = harness.stacks()[&harness.tabs.active()].clone();
    let Harness { tabs, showing, .. } = harness;

    let detached = tabs.into_detached(|| showing.clone());
    let (tabs, place) = Tabs::detached(detached);

    assert_eq!(place, showing);
    assert_eq!(tabs.count(), 1);
    tabs.check();
    let harness = Harness {
        tabs,
        ..Harness::new()
    };
    assert_eq!(harness.stacks()[&harness.tabs.active()], stacks);
}

#[test]
#[should_panic(expected = "one tab")]
fn into_detached_panics_on_two_tabs() {
    let mut harness = Harness::new();
    harness.insert(1);
    let _ = harness.tabs.into_detached(Fake::default);
}

proptest! {
    #[test]
    fn take_and_put_across_two_groups_keep_every_tab(
        ops_a in ops(),
        ops_b in ops(),
        moves in proptest::collection::vec((any::<bool>(), any::<Index>(), 0..12_usize), 1..16),
    ) {
        let mut harnesses = [run(&ops_a, |_, _, _, _, _| {}), run(&ops_b, |_, _, _, _, _| {})];
        for (forth, ix, at) in moves {
            let [a, b] = &mut harnesses;
            let (from, to) = if forth { (a, b) } else { (b, a) };
            if from.tabs.count() == 1 {
                continue;
            }
            let id = from.pick(ix);
            let stacks = from.stacks()[&id].clone();
            let mut notes = [from.notes(), to.notes()].concat();
            notes.sort_unstable();
            let count = from.tabs.count() + to.tabs.count();

            let detached = from.take(id).expect("a picked tab");
            let put = to.put(detached, at);

            from.tabs.check();
            to.tabs.check();
            let mut after = [from.notes(), to.notes()].concat();
            after.sort_unstable();
            prop_assert_eq!(after, notes);
            prop_assert_eq!(from.tabs.count() + to.tabs.count(), count);
            prop_assert_eq!(&to.stacks()[&put], &stacks);
            prop_assert!(from.tabs.iter().all(|tab| tab.id() != id), "the tab left");
        }
    }
}
