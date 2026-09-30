//! Closing a tab, committing an open, and the closed stack `reopen` pops (tabs spec, A1 invariants 7, 8).

use crate::history::{HISTORY, Stack};
use crate::{Place, Tab, TabId, Tabs};

#[derive(Debug, Clone, PartialEq)]
pub struct Closed<P> {
    /// The tab's place, fresh from `leaving` if it was active.
    pub place: P,
    /// Where it stood; an insert there puts it back.
    pub index: usize,
    /// The tab now active, when the active tab closed.
    pub shown: Option<(TabId, P)>,
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

impl<P> Tabs<P> {
    /// Whether `reopen` has a tab to bring back.
    #[must_use]
    pub fn has_closed(&self) -> bool {
        !self.closed.is_empty()
    }
}

impl<P: Place> Tabs<P> {
    /// The last tab gives way to an empty one with a fresh id: a load keyed by the old id lands nowhere.
    ///
    /// # Panics
    /// Only on a broken invariant (H8): an inactive tab always holds a place.
    pub fn close(
        &mut self,
        id: TabId,
        leaving: impl FnOnce() -> P,
        resolve: impl FnOnce(P) -> P,
    ) -> Option<Closed<P>> {
        let index = self.index_of(id)?;
        let was_active = id == self.active;
        let left = was_active.then(|| self.leave_active(leaving).1);
        let tab = self.list.remove(index);
        let place = match (left, tab.place) {
            (Some(left), _) => left,
            (None, Some(stored)) => resolve(stored),
            (None, None) => panic!("H8: an inactive tab holds a place"),
        };
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
                place: Some(P::default()),
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

    /// Leaves tab `id` if it is still active: the place left goes on back and forward clears.
    #[must_use]
    pub fn commit(&mut self, id: TabId, leaving: impl FnOnce() -> P) -> bool {
        if id != self.active {
            return false;
        }
        let (tab, left) = self.leave_active(leaving);
        tab.back.push(left);
        tab.forward.clear();
        true
    }

    /// Reopens the newest closed tab at its old index under a fresh id; `None`, calling no `leaving`, if none.
    pub fn reopen(&mut self, leaving: impl FnOnce() -> P) -> Option<P> {
        let entry = self.closed.pop_back()?;
        self.insert(entry.index, leaving);
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
