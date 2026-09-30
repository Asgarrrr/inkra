//! Tabs with stable ids, an MRU and the index rule, generic over the place a tab shows (tabs spec, A1).

use std::collections::VecDeque;

/// What a tab shows, as far as history needs it.
pub trait Place: Default + Clone {
    type Note: Copy + Eq;
    /// `None`: an empty tab. Never pushed on a stack (invariant 5).
    fn note(&self) -> Option<Self::Note>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

/// Each back, forward and closed stack keeps at most this many entries (invariant 6).
const HISTORY: usize = 1024;

/// The stack a walk pops from, then the stack it pushes the place left on.
type Walk<P> = fn(&mut Tab<P>) -> (&mut Stack<P>, &mut Stack<P>);

/// Oldest first; no empty place (H2) and no two adjacent entries on one note (H1).
#[derive(Debug, Clone)]
struct Stack<P>(VecDeque<P>);

impl<P> Default for Stack<P> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<P: Place> Stack<P> {
    /// Skips an empty place; replaces a top on the same note, else drops the oldest when full.
    fn push(&mut self, place: P) {
        let Some(note) = place.note() else { return };
        if self.0.back().and_then(Place::note) == Some(note) {
            self.0.pop_back();
        } else if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(place);
    }

    /// Re-pushes every entry not on `note`: the push rule collapses the runs a removal joined, keeping the newest.
    fn forget(&mut self, note: P::Note) {
        for place in std::mem::take(&mut self.0) {
            if place.note() != Some(note) {
                self.push(place);
            }
        }
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    /// The newest entry off the note `showing`; the entries on it above that one are dropped.
    fn pop(&mut self, showing: Option<P::Note>) -> Option<P> {
        std::iter::from_fn(|| self.0.pop_back()).find(|place| place.note() != showing)
    }
}

#[derive(Debug, Clone)]
pub struct Tab<P> {
    id: TabId,
    /// `None` exactly for the active tab (H8): its place is on screen, not here.
    place: Option<P>,
    back: Stack<P>,
    forward: Stack<P>,
}

impl<P> Tab<P> {
    fn new(id: TabId) -> Self {
        Self {
            id,
            place: None,
            back: Stack::default(),
            forward: Stack::default(),
        }
    }

    #[must_use]
    pub fn id(&self) -> TabId {
        self.id
    }

    /// `None` for the active tab.
    #[must_use]
    pub fn place(&self) -> Option<&P> {
        self.place.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

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
struct ClosedTab<P> {
    place: P,
    index: usize,
    back: Stack<P>,
    forward: Stack<P>,
}

impl<P: Place> ClosedTab<P> {
    /// Nothing to reopen: no note and no history (invariant 7).
    fn is_blank(&self) -> bool {
        self.place.note().is_none() && self.back.0.is_empty() && self.forward.0.is_empty()
    }
}

#[derive(Debug)]
pub struct Tabs<P> {
    list: Vec<Tab<P>>,
    active: TabId,
    mru: Vec<TabId>,
    next: u64,
    /// Oldest first.
    closed: VecDeque<ClosedTab<P>>,
}

impl<P> Default for Tabs<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> Tabs<P> {
    /// One active tab; the caller shows its place.
    #[must_use]
    pub fn new() -> Self {
        let id = TabId(0);
        Self {
            list: vec![Tab::new(id)],
            active: id,
            mru: vec![id],
            next: 1,
            closed: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn active(&self) -> TabId {
        self.active
    }

    /// # Panics
    /// Only on a broken invariant: the active id is always a tab.
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.index_of(self.active)
            .unwrap_or_else(|| panic!("the active id is always a tab"))
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.list.len()
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Tab<P>> {
        self.list.iter()
    }

    #[must_use]
    pub fn id_at(&self, index: usize) -> Option<TabId> {
        self.list.get(index).map(Tab::id)
    }

    /// # Panics
    /// Only on a broken invariant: there is always a tab.
    #[must_use]
    pub fn last(&self) -> TabId {
        self.list
            .last()
            .unwrap_or_else(|| panic!("there is always a tab"))
            .id
    }

    /// The tab beside the active one, wrapping at either end.
    #[must_use]
    pub fn step(&self, direction: Direction) -> TabId {
        let count = self.list.len();
        let index = match direction {
            Direction::Next => self.active_index() + 1,
            Direction::Previous => self.active_index() + count - 1,
        };
        self.list[index % count].id
    }

    /// Moves tab `id` to `to`, clamped to the bar; the MRU and the active id stay (invariant 3).
    /// Nothing for an id that is no tab: it closed during the drag.
    pub fn move_tab(&mut self, id: TabId, to: usize) {
        let Some(from) = self.index_of(id) else {
            return;
        };
        let tab = self.list.remove(from);
        self.list.insert(to.min(self.list.len()), tab);
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.list.iter().position(|tab| tab.id == id)
    }
}

impl<P: Place> Tabs<P> {
    /// Opens a new tab at `at`, clamped to the bar, and activates it; the caller shows its place.
    pub fn insert(&mut self, at: usize, leaving: impl FnOnce() -> P) -> TabId {
        let (tab, left) = self.leave_active(leaving);
        tab.place = Some(left);
        let id = self.fresh_id();
        self.list.insert(at.min(self.list.len()), Tab::new(id));
        self.active = id;
        self.mru.insert(0, id);
        id
    }

    /// Returns the arriving tab's place, or `None` if `id` is active or no tab.
    pub fn activate(&mut self, id: TabId, leaving: impl FnOnce() -> P) -> Option<P> {
        if id == self.active {
            return None;
        }
        let index = self.index_of(id)?;
        let (tab, left) = self.leave_active(leaving);
        tab.place = Some(left);
        self.active = id;
        self.mru.retain(|&other| other != id);
        self.mru.insert(0, id);
        self.list[index].place.take()
    }

    /// The last tab gives way to an empty one with a fresh id: a load keyed by the old id lands nowhere.
    /// The closed stack keeps the tab's place: `leaving()` if it was active, else `resolve` of the
    /// place it stored.
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

    /// Leaves tab `id` for the place the caller shows next, if it is still active: the place left
    /// goes on back and forward clears. Else changes nothing.
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

    /// The newest place behind the active tab on another note than `showing`; `None`, calling no
    /// `leaving`, when there is none. The place left goes on forward.
    pub fn back(&mut self, showing: Option<P::Note>, leaving: impl FnOnce() -> P) -> Option<P> {
        self.walk(showing, leaving, |tab| (&mut tab.back, &mut tab.forward))
    }

    /// As `back`, the other way.
    pub fn forward(&mut self, showing: Option<P::Note>, leaving: impl FnOnce() -> P) -> Option<P> {
        self.walk(showing, leaving, |tab| (&mut tab.forward, &mut tab.back))
    }

    /// Reinserts the newest closed tab at its old index, clamped, under a fresh id, with its
    /// history, and activates it; returns its place. `None`, calling no `leaving`, when nothing is closed.
    pub fn reopen(&mut self, leaving: impl FnOnce() -> P) -> Option<P> {
        let entry = self.closed.pop_back()?;
        self.insert(entry.index, leaving);
        let index = self.active_index();
        let tab = &mut self.list[index];
        tab.back = entry.back;
        tab.forward = entry.forward;
        Some(entry.place)
    }

    /// Empties every inactive tab and closed tab on `note` and drops it from every stack; the
    /// active tab's place is the caller's. A closed tab left blank drops (invariant 7).
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

    // The one caller of `leaving`: it may have side effects, so it runs once per real leave.
    fn leave_active(&mut self, leaving: impl FnOnce() -> P) -> (&mut Tab<P>, P) {
        let index = self.active_index();
        (&mut self.list[index], leaving())
    }

    fn fresh_id(&mut self) -> TabId {
        let id = TabId(self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests;
