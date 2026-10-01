//! Closing a tab, committing an open, and the closed stack `reopen` pops (tabs spec, A1 invariants 7, 8).

use crate::history::{HISTORY, Stack};
use crate::{Place, Tab, TabId, Tabs};

#[derive(Debug, Clone, PartialEq)]
pub struct Closed<P, Q = P> {
    /// The plain place the closed stack keeps, from `closing`.
    pub place: P,
    /// Where it stood; an insert there puts it back.
    pub index: usize,
    /// The tab now active and the place it held, when the active tab closed.
    pub shown: Option<(TabId, Q)>,
}

/// A closed tab as `reopen` puts it back (A1 invariants 7, 8).
#[derive(Debug, Clone)]
pub(crate) struct ClosedTab<P> {
    pub(crate) place: P,
    pub(crate) index: usize,
    pub(crate) back: Stack<P>,
    pub(crate) forward: Stack<P>,
}

impl<P: Place> ClosedTab<P> {
    /// Nothing to reopen: no note and no history (invariant 7).
    pub(crate) fn is_blank(&self) -> bool {
        self.place.note().is_none() && self.back.0.is_empty() && self.forward.0.is_empty()
    }
}

impl<P, Q> Tabs<P, Q> {
    /// Whether `reopen` has a tab to bring back.
    #[must_use]
    pub fn has_closed(&self) -> bool {
        !self.closed.is_empty()
    }
}

impl<P: Place + Clone, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// `closing` gets the closed tab's slot, `None` for the active tab (H8), and returns the plain
    /// place the closed stack keeps; it runs once, after the tab has left the list, so it must not
    /// look the tab up. The last tab gives way to an empty one with a fresh id: a load keyed by the
    /// old id lands nowhere.
    ///
    /// # Panics
    /// On a broken H8, before `closing` runs: an inactive tab with no slot, or the active tab with
    /// one.
    pub fn close(
        &mut self,
        id: TabId,
        closing: impl FnOnce(Option<Q>) -> P,
    ) -> Option<Closed<P, Q>> {
        let index = self.index_of(id)?;
        let was_active = id == self.active;
        let tab = self.list.remove(index);
        let slot = match (was_active, tab.place) {
            (true, None) => None,
            (false, Some(parked)) => Some(parked),
            _ => panic!("H8: a tab's slot is empty exactly when it is active"),
        };
        let place = closing(slot);
        self.remember(ClosedTab {
            place: place.clone(),
            index,
            back: tab.back,
            forward: tab.forward,
        });
        self.mru.retain(|&other| other != id);
        if self.list.is_empty() {
            let fresh = self.fresh_id();
            self.list.push(Tab {
                place: Some(Q::default()),
                ..Tab::new(fresh)
            });
            self.mru.push(fresh);
        }
        let shown = was_active.then(|| {
            self.active = self.mru[0];
            let index = self.active_index();
            let place = self.list[index]
                .place
                .take()
                .unwrap_or_else(|| panic!("H8: the tab taking over held no place"));
            (self.active, place)
        });
        Some(Closed {
            place,
            index,
            shown,
        })
    }
}

impl<P: Place, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// Leaves tab `id` if it is still active: the place left goes on back and forward clears.
    #[must_use]
    pub fn commit(&mut self, id: TabId, capture: impl FnOnce() -> P) -> bool {
        if id != self.active {
            return false;
        }
        let (tab, left) = self.leave_active(capture);
        tab.back.push(left);
        tab.forward.clear();
        true
    }

    /// Reopens the newest closed tab at its old index under a fresh id; `None`, calling no `park`,
    /// if none. The tab left keeps what `park` returns.
    pub fn reopen(&mut self, park: impl FnOnce() -> Q) -> Option<P> {
        let entry = self.closed.pop_back()?;
        self.insert(entry.index, park);
        let index = self.active_index();
        let tab = &mut self.list[index];
        tab.back = entry.back;
        tab.forward = entry.forward;
        Some(entry.place)
    }

    /// Skips a blank entry (invariant 7); drops the oldest when full (invariant 6).
    fn remember(&mut self, entry: ClosedTab<P>) {
        if entry.is_blank() {
            return;
        }
        if self.closed.len() == HISTORY {
            self.closed.pop_front();
        }
        self.closed.push_back(entry);
    }
}
