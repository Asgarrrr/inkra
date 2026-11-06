//! Per-tab back and forward stacks, and dropping a note from all history (tabs spec, A1 invariants 4–6 and 9).

use std::collections::VecDeque;
use std::iter::successors;
use std::num::NonZeroUsize;

use crate::{Place, Tab, Tabs};

/// Each back, forward and closed stack keeps at most this many entries (invariant 6).
pub(crate) const HISTORY: usize = 1024;

/// Which way a walk goes through the active tab's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Toward older places: pops the back stack.
    Back,
    /// Toward newer places: pops the forward stack.
    Forward,
}

impl Way {
    /// The stack a walk pops from, then the stack it pushes the place left on.
    fn stacks<P, Q>(self, tab: &mut Tab<P, Q>) -> (&mut Stack<P>, &mut Stack<P>) {
        match self {
            Self::Back => (&mut tab.back, &mut tab.forward),
            Self::Forward => (&mut tab.forward, &mut tab.back),
        }
    }

    /// The stack a walk pops from.
    fn from<P, Q>(self, tab: &Tab<P, Q>) -> &Stack<P> {
        match self {
            Self::Back => &tab.back,
            Self::Forward => &tab.forward,
        }
    }
}

/// Where `steps` successive walks `way` arrive: a place on `note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop<N> {
    pub way: Way,
    pub steps: NonZeroUsize,
    pub note: N,
}

/// Oldest first; no empty place (H2) and no two adjacent entries on one note (H1).
#[derive(Debug, Clone)]
pub(crate) struct Stack<P>(pub(crate) VecDeque<P>);

impl<P> Default for Stack<P> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<P: Place> Stack<P> {
    /// Skips an empty place; replaces a top on the same note, else drops the oldest when full.
    pub(crate) fn push(&mut self, place: P) {
        let Some(note) = place.note() else { return };
        if self.0.back().and_then(Place::note) == Some(note) {
            self.0.pop_back();
        } else if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(place);
    }

    /// Re-pushes every entry not on `note`: the push rule collapses the runs a removal joined, keeping the newest.
    pub(crate) fn forget(&mut self, note: P::Note) {
        for place in std::mem::take(&mut self.0) {
            if place.note() != Some(note) {
                self.push(place);
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// The newest entry off the note `showing`; the entries on it above that one are dropped.
    pub(crate) fn pop(&mut self, showing: Option<P::Note>) -> Option<P> {
        std::iter::from_fn(|| self.0.pop_back()).find(|place| place.note() != showing)
    }
}

impl<P: Place, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// The newest place behind on another note than `showing`; `None`, calling no `capture`, if
    /// none.
    pub fn back(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P> {
        self.walk(Way::Back, NonZeroUsize::MIN, showing, capture)
    }

    /// As `back`, the other way.
    pub fn forward(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P> {
        self.walk(Way::Forward, NonZeroUsize::MIN, showing, capture)
    }

    /// Whether `back(showing, _)` arrives; it reads and captures nothing.
    #[must_use]
    pub fn can_go_back(&self, showing: Option<P::Note>) -> bool {
        self.stops(Way::Back, showing).next().is_some()
    }

    /// As `can_go_back`, the other way.
    #[must_use]
    pub fn can_go_forward(&self, showing: Option<P::Note>) -> bool {
        self.stops(Way::Forward, showing).next().is_some()
    }

    /// Where successive walks `way` from `showing` arrive, nearest first; the n-th is the n-th walk.
    pub fn stops(
        &self,
        way: Way,
        showing: Option<P::Note>,
    ) -> impl Iterator<Item = Stop<P::Note>> + '_ {
        way.from(&self.list[self.active_index()])
            .0
            .iter()
            .rev()
            .filter_map(Place::note)
            .skip_while(move |&note| Some(note) == showing)
            .zip(successors(Some(NonZeroUsize::MIN), |n| n.checked_add(1)))
            .map(move |(note, steps)| Stop { way, steps, note })
    }

    /// Arrives where `stop.steps` walks would, capturing once (invariant 9); `None`, calling no
    /// `capture` and changing no stack, if the walks stop short or arrive off `stop.note`.
    pub fn walk_to(
        &mut self,
        stop: Stop<P::Note>,
        showing: Option<P::Note>,
        capture: impl FnOnce() -> P,
    ) -> Option<P> {
        self.stops(stop.way, showing)
            .nth(stop.steps.get() - 1)
            .filter(|at| at.note == stop.note)?;
        self.walk(stop.way, stop.steps, showing, capture)
    }

    /// Drops `note` from every inactive tab, closed tab and stack; the active tab's place is the caller's.
    pub fn forget(&mut self, note: P::Note) {
        for tab in &mut self.list {
            if let Some(place) = &mut tab.place
                && place.note() == Some(note)
            {
                *place = Q::default();
            }
            tab.back.forget(note);
            tab.forward.forget(note);
        }
        for entry in &mut self.closed {
            if entry.place.note() == Some(note) {
                entry.place = P::default();
            }
            entry.back.forget(note);
            entry.forward.forget(note);
        }
        self.closed.retain(|entry| !entry.is_blank());
    }

    // Pops before `capture` runs: it seals an undo group, so it runs only when a place arrives.
    fn walk(
        &mut self,
        way: Way,
        steps: NonZeroUsize,
        showing: Option<P::Note>,
        capture: impl FnOnce() -> P,
    ) -> Option<P> {
        let index = self.active_index();
        let (from, _) = way.stacks(&mut self.list[index]);
        let mut arriving = from.pop(showing)?;
        // H1: past the first pop, `pop_back` is the pop the next walk would make.
        let passed: Vec<P> = std::iter::from_fn(|| from.0.pop_back())
            .take(steps.get() - 1)
            .map(|next| std::mem::replace(&mut arriving, next))
            .collect();
        let (tab, left) = self.leave_active(capture);
        let (_, to) = way.stacks(tab);
        to.push(left);
        for place in passed {
            to.push(place);
        }
        Some(arriving)
    }
}
