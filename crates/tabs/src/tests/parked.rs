use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use proptest::prelude::*;

use super::{Fake, Op, ops};
use crate::{Place, Tab, TabId, Tabs};

/// An inactive tab's value, apart from the plain places history keeps; not `Clone`, so it cannot be
/// duplicated.
#[derive(Debug, Default, PartialEq)]
struct Kept {
    note: Option<u8>,
    serial: u32,
}

impl Place for Kept {
    type Note = u8;

    fn note(&self) -> Option<u8> {
        self.note
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Calls {
    park: u32,
    capture: u32,
    closing: u32,
}

/// `Tabs<Fake, Kept>` whose `park` hands out a fresh serial and records it against the tab left.
struct Driver {
    tabs: Tabs<Fake, Kept>,
    showing: Option<u8>,
    serial: u32,
    /// Each inactive tab's `Kept`, as last left or restored.
    recorded: HashMap<TabId, Kept>,
    calls: Calls,
}

fn parking(showing: Option<u8>, serial: &mut u32, calls: &mut Calls) -> impl FnOnce() -> Kept {
    move || {
        calls.park += 1;
        *serial += 1;
        Kept {
            note: showing,
            serial: *serial,
        }
    }
}

fn capturing(showing: Option<u8>, calls: &mut Calls) -> impl FnOnce() -> Fake {
    move || {
        calls.capture += 1;
        Fake {
            note: showing,
            seq: 0,
        }
    }
}

impl Driver {
    fn new(start: Option<(Vec<Option<u8>>, usize)>) -> Self {
        let Some((notes, active)) = start else {
            return Self {
                tabs: Tabs::new(),
                showing: None,
                serial: 0,
                recorded: HashMap::new(),
                calls: Calls::default(),
            };
        };
        let kept = (1..)
            .zip(&notes)
            .map(|(serial, &note)| Kept { note, serial });
        let tabs = Tabs::<Fake, Kept>::restored(kept.collect(), active);
        let recorded = (1..)
            .zip(tabs.iter().zip(&notes))
            .filter(|(_, (tab, _))| tab.id() != tabs.active())
            .map(|(serial, (tab, &note))| (tab.id(), Kept { note, serial }))
            .collect();
        Self {
            showing: notes[active],
            serial: u32::try_from(notes.len()).unwrap(),
            tabs,
            recorded,
            calls: Calls::default(),
        }
    }

    /// The `Kept` recorded for `id`; a tab never parked holds the default.
    fn take(&mut self, id: TabId) -> Kept {
        self.recorded.remove(&id).unwrap_or_default()
    }

    fn pick(&self, ix: proptest::sample::Index) -> TabId {
        let ids: Vec<TabId> = self.tabs.iter().map(Tab::id).collect();
        ids[ix.index(ids.len())]
    }

    /// Runs `op`, checks K2 on what it returns and K3 on the closures it called, then K1.
    fn apply(&mut self, op: &Op) {
        let (left, showing, before) = (self.tabs.active(), self.showing, self.calls);
        let mut expected = before;
        match op {
            Op::Insert(at) => {
                let park = parking(showing, &mut self.serial, &mut self.calls);
                self.tabs.insert(*at, park);
                expected.park += 1;
                self.showing = None;
            }
            Op::Activate(ix) => {
                let id = self.pick(*ix);
                self.activate(id, &mut expected);
            }
            Op::Step(direction) => self.activate(self.tabs.step(*direction), &mut expected),
            Op::Last => self.activate(self.tabs.last(), &mut expected),
            Op::Close(ix) => {
                let id = self.pick(*ix);
                self.close(id, &mut expected);
            }
            Op::Commit(ix, note) => {
                let id = self.pick(*ix);
                let capture = capturing(showing, &mut self.calls);
                if self.tabs.commit(id, capture) {
                    self.showing = *note;
                }
                expected.capture += u32::from(id == left);
            }
            Op::Back | Op::Forward => {
                let capture = capturing(showing, &mut self.calls);
                let arrived = if matches!(op, Op::Back) {
                    self.tabs.back(showing, capture)
                } else {
                    self.tabs.forward(showing, capture)
                };
                if let Some(place) = arrived {
                    expected.capture += 1;
                    self.showing = place.note;
                }
            }
            Op::Reopen => {
                let entry = self.tabs.has_closed();
                let park = parking(showing, &mut self.serial, &mut self.calls);
                let arrived = self.tabs.reopen(park);
                assert_eq!(arrived.is_some(), entry, "a reopen finds the entry");
                if let Some(place) = arrived {
                    expected.park += 1;
                    self.showing = place.note;
                }
            }
            Op::Move(ix, to) => {
                let id = self.pick(*ix);
                self.tabs.move_tab(id, *to);
            }
            Op::Forget(note) => {
                self.tabs.forget(*note);
                for kept in self.recorded.values_mut() {
                    if kept.note == Some(*note) {
                        *kept = Kept::default();
                    }
                }
                if self.showing == Some(*note) {
                    self.showing = None;
                }
            }
        }
        assert_eq!(self.calls, expected, "K3: {op:?}");
        if self.calls.park > before.park {
            let serial = self.serial;
            self.recorded.insert(
                left,
                Kept {
                    note: showing,
                    serial,
                },
            );
        }
        self.tabs.check();
        self.check_slots(op);
    }

    /// K1.
    fn check_slots(&self, op: &Op) {
        for tab in self
            .tabs
            .iter()
            .filter(|tab| tab.id() != self.tabs.active())
        {
            let recorded = self.recorded.get(&tab.id());
            assert!(
                recorded.is_some(),
                "K1: {op:?} left an unrecorded tab inactive"
            );
            assert_eq!(tab.place(), recorded, "K1: {op:?}");
        }
    }

    fn activate(&mut self, id: TabId, expected: &mut Calls) {
        let left = self.tabs.active();
        let park = parking(self.showing, &mut self.serial, &mut self.calls);
        let arrived = self.tabs.activate(id, park);
        if id == left {
            assert_eq!(arrived, None, "activating the active tab returns nothing");
            return;
        }
        expected.park += 1;
        let arrived = arrived.expect("an inactive tab arrives");
        assert_eq!(
            arrived,
            self.take(id),
            "K2: `activate` returns the tab's slot"
        );
        self.showing = arrived.note;
    }

    fn close(&mut self, id: TabId, expected: &mut Calls) {
        let left = self.tabs.active();
        let calls = &mut self.calls;
        let mut handed = None;
        let closed = self.tabs.close(id, |slot| {
            calls.closing += 1;
            handed = Some(slot);
            Fake::default()
        });
        let closed = closed.expect("a tab closes");
        expected.closing += 1;
        let slot = (id != left).then(|| self.take(id));
        assert_eq!(
            handed,
            Some(slot),
            "K2: `closing` gets the closed tab's slot"
        );
        match closed.shown {
            Some((shown, kept)) => {
                assert_eq!(id, left, "only closing the active tab shows another");
                assert_eq!(kept, self.take(shown), "K2: `shown` is the tab's slot");
                self.showing = kept.note;
            }
            None => assert_ne!(id, left, "closing the active tab shows another"),
        }
    }
}

fn restore() -> impl Strategy<Value = (Vec<Option<u8>>, usize)> {
    proptest::collection::vec(proptest::option::of(0..4_u8), 1..8).prop_flat_map(|notes| {
        let len = notes.len();
        (Just(notes), 0..len)
    })
}

proptest! {
    #[test]
    fn an_inactive_tab_holds_the_value_it_was_left_with(
        start in proptest::option::of(restore()),
        ops in ops(),
    ) {
        let mut driver = Driver::new(start);
        for op in &ops {
            driver.apply(op);
        }
    }
}

#[test]
fn close_panics_on_a_broken_h8() {
    for break_active in [false, true] {
        let mut tabs = Tabs::<Fake, Kept>::new();
        let left = tabs.active();
        tabs.insert(1, Kept::default);
        let id = if break_active {
            tabs.list[1].place = Some(Kept::default());
            tabs.active()
        } else {
            tabs.list[0].place = None;
            left
        };
        let closing = Cell::new(false);

        let panic = catch_unwind(AssertUnwindSafe(|| {
            tabs.close(id, |_| {
                closing.set(true);
                Fake::default()
            })
        }))
        .expect_err("a broken H8 panics");

        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        assert!(message.contains("H8"), "{message}");
        assert!(!closing.get(), "the panic comes before `closing`");
    }
}
