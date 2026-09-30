//! Per-tab back and forward stacks, and dropping a note from all history (tabs spec, A1 invariants 4–6).

use std::collections::VecDeque;

use crate::{Place, Tab, Tabs};

/// Each back, forward and closed stack keeps at most this many entries (invariant 6).
pub(crate) const HISTORY: usize = 1024;

/// The stack a walk pops from, then the stack it pushes the place left on.
type Walk<P> = fn(&mut Tab<P>) -> (&mut Stack<P>, &mut Stack<P>);

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

impl<P: Place> Tabs<P> {
    /// The newest place behind on another note than `showing`; `None`, calling no `leaving`, if none.
    pub fn back(&mut self, showing: Option<P::Note>, leaving: impl FnOnce() -> P) -> Option<P> {
        self.walk(showing, leaving, |tab| (&mut tab.back, &mut tab.forward))
    }

    /// As `back`, the other way.
    pub fn forward(&mut self, showing: Option<P::Note>, leaving: impl FnOnce() -> P) -> Option<P> {
        self.walk(showing, leaving, |tab| (&mut tab.forward, &mut tab.back))
    }

    /// Drops `note` from every inactive tab, closed tab and stack; the active tab's place is the caller's.
    pub fn forget(&mut self, note: P::Note) {
        for tab in &mut self.list {
            if let Some(place) = &mut tab.place
                && place.note() == Some(note)
            {
                *place = P::default();
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

    // Pops before `leaving` runs: it seals an undo group, so it runs only when a place arrives.
    fn walk(
        &mut self,
        showing: Option<P::Note>,
        leaving: impl FnOnce() -> P,
        stacks: Walk<P>,
    ) -> Option<P> {
        let index = self.active_index();
        let arriving = stacks(&mut self.list[index]).0.pop(showing)?;
        let (tab, left) = self.leave_active(leaving);
        stacks(tab).1.push(left);
        Some(arriving)
    }
}
