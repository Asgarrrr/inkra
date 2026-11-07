//! Per-tab back and forward stacks, and dropping a file from all history (tabs spec, A1 invariants 4–6 and 9).

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

/// Where `steps` successive walks `way` arrive: a place on `file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop<N> {
    pub way: Way,
    pub steps: NonZeroUsize,
    pub file: N,
}

/// Oldest first; no empty place (H2) and no two adjacent entries on one file (H1).
#[derive(Debug, Clone)]
pub(crate) struct Stack<P>(pub(crate) VecDeque<P>);

impl<P> Default for Stack<P> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<P: Place> Stack<P> {
    /// Skips an empty place; replaces a top on the same file, else drops the oldest when full.
    pub(crate) fn push(&mut self, place: P) {
        let Some(file) = place.file() else { return };
        if self.0.back().and_then(Place::file) == Some(file) {
            self.0.pop_back();
        } else if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(place);
    }

    /// Re-pushes every entry not on `file`: the push rule collapses the runs a removal joined, keeping the newest.
    pub(crate) fn forget(&mut self, file: P::File) {
        for place in std::mem::take(&mut self.0) {
            if place.file() != Some(file) {
                self.push(place);
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// The newest entry off the file `showing`; the entries on it above that one are dropped.
    pub(crate) fn pop(&mut self, showing: Option<P::File>) -> Option<P> {
        std::iter::from_fn(|| self.0.pop_back()).find(|place| place.file() != showing)
    }
}

/// The files on `back` then `forward`.
fn stacked<'a, P: Place>(
    back: &'a Stack<P>,
    forward: &'a Stack<P>,
) -> impl Iterator<Item = P::File> + 'a {
    back.0.iter().chain(&forward.0).filter_map(Place::file)
}

impl<P: Place, Q: Place<File = P::File>> Tabs<P, Q> {
    /// The newest place behind on another file than `showing`; `None`, calling no `capture`, if
    /// none.
    pub fn back(&mut self, showing: Option<P::File>, capture: impl FnOnce() -> P) -> Option<P> {
        self.walk(Way::Back, NonZeroUsize::MIN, showing, capture)
    }

    /// As `back`, the other way.
    pub fn forward(&mut self, showing: Option<P::File>, capture: impl FnOnce() -> P) -> Option<P> {
        self.walk(Way::Forward, NonZeroUsize::MIN, showing, capture)
    }

    /// Whether `back(showing, _)` arrives; it reads and captures nothing.
    #[must_use]
    pub fn can_go_back(&self, showing: Option<P::File>) -> bool {
        self.stops(Way::Back, showing).next().is_some()
    }

    /// As `can_go_back`, the other way.
    #[must_use]
    pub fn can_go_forward(&self, showing: Option<P::File>) -> bool {
        self.stops(Way::Forward, showing).next().is_some()
    }

    /// Where successive walks `way` from `showing` arrive, nearest first; the n-th is the n-th walk.
    pub fn stops(
        &self,
        way: Way,
        showing: Option<P::File>,
    ) -> impl Iterator<Item = Stop<P::File>> + '_ {
        way.from(&self.list[self.active_index()])
            .0
            .iter()
            .rev()
            .filter_map(Place::file)
            .skip_while(move |&file| Some(file) == showing)
            .zip(successors(Some(NonZeroUsize::MIN), |n| n.checked_add(1)))
            .map(move |(file, steps)| Stop { way, steps, file })
    }

    /// Arrives where `stop.steps` walks would, capturing once (invariant 9); `None`, calling no
    /// `capture` and changing no stack, if the walks stop short or arrive off `stop.file`.
    pub fn walk_to(
        &mut self,
        stop: Stop<P::File>,
        showing: Option<P::File>,
        capture: impl FnOnce() -> P,
    ) -> Option<P> {
        self.stops(stop.way, showing)
            .nth(stop.steps.get() - 1)
            .filter(|at| at.file == stop.file)?;
        self.walk(stop.way, stop.steps, showing, capture)
    }

    /// Drops `file` from every inactive tab, closed tab and stack; the active tab's place is the caller's.
    pub fn forget(&mut self, file: P::File) {
        for tab in &mut self.list {
            if let Some(place) = &mut tab.place
                && place.file() == Some(file)
            {
                *place = Q::default();
            }
            tab.back.forget(file);
            tab.forward.forget(file);
        }
        for entry in &mut self.closed {
            if entry.place.file() == Some(file) {
                entry.place = P::default();
            }
            entry.back.forget(file);
            entry.forward.forget(file);
        }
        self.closed.retain(|entry| !entry.is_blank());
    }

    /// The files `forget` would drop, with duplicates; never the active tab's place (H8).
    pub fn files(&self) -> impl Iterator<Item = P::File> + '_ {
        let tabs = self.list.iter().flat_map(|tab| {
            let place = tab.place.as_ref().and_then(Place::file);
            place.into_iter().chain(stacked(&tab.back, &tab.forward))
        });
        let closed = self.closed.iter().flat_map(|entry| {
            let place = entry.place.file();
            place
                .into_iter()
                .chain(stacked(&entry.back, &entry.forward))
        });
        tabs.chain(closed)
    }

    // Pops before `capture` runs: it seals an undo group, so it runs only when a place arrives.
    fn walk(
        &mut self,
        way: Way,
        steps: NonZeroUsize,
        showing: Option<P::File>,
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
