//! Tabs with stable ids, an MRU and the index rule, generic over the place a tab shows (tabs spec, A1).

mod closed;
mod history;

use std::collections::VecDeque;

pub use closed::Closed;
use closed::ClosedTab;
#[cfg(test)]
use history::HISTORY;
use history::Stack;

/// What a tab shows, as far as history needs it.
pub trait Place: Default {
    type Note: Copy + Eq;
    /// `None`: an empty tab. Never pushed on a stack (invariant 5).
    fn note(&self) -> Option<Self::Note>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

/// Its stacks hold plain places `P`; its slot, while inactive, holds `Q`.
#[derive(Debug, Clone)]
pub struct Tab<P, Q = P> {
    id: TabId,
    /// `None` exactly for the active tab (H8): its place is on screen, not here.
    place: Option<Q>,
    back: Stack<P>,
    forward: Stack<P>,
}

impl<P, Q> Tab<P, Q> {
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

    /// `None` for the active tab (H8): its place is on screen.
    #[must_use]
    pub fn place(&self) -> Option<&Q> {
        self.place.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

#[derive(Debug)]
pub struct Tabs<P, Q = P> {
    list: Vec<Tab<P, Q>>,
    active: TabId,
    mru: Vec<TabId>,
    next: u64,
    /// Oldest first.
    closed: VecDeque<ClosedTab<P>>,
}

impl<P, Q> Default for Tabs<P, Q> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P, Q> Tabs<P, Q> {
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
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Tab<P, Q>> {
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

    /// Moves tab `id` to `to`, clamped; the MRU and the active id stay (invariant 3). A closed `id` is a no-op.
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

impl<P: Place, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// Tabs on `places` in bar order; the caller shows `active`'s (H8), and its `Q` is dropped.
    /// Fresh ids, MRU the active first.
    ///
    /// # Panics
    /// If `places` is empty or `active` is out of range.
    #[must_use]
    pub fn restored(places: Vec<Q>, active: usize) -> Self {
        assert!(
            active < places.len(),
            "active tab {active} out of range of {} places",
            places.len()
        );
        let mut tabs = Self {
            list: Vec::with_capacity(places.len()),
            active: TabId(0),
            mru: Vec::with_capacity(places.len()),
            next: 0,
            closed: VecDeque::new(),
        };
        for place in places {
            let id = tabs.fresh_id();
            tabs.list.push(Tab {
                place: Some(place),
                ..Tab::new(id)
            });
        }
        let shown = &mut tabs.list[active];
        shown.place = None;
        tabs.active = shown.id;
        tabs.mru = std::iter::once(tabs.active)
            .chain(
                tabs.list
                    .iter()
                    .map(Tab::id)
                    .filter(|&id| id != tabs.active),
            )
            .collect();
        tabs
    }

    /// Opens a new tab at `at`, clamped to the bar, and activates it; the tab left keeps what
    /// `park` returns.
    pub fn insert(&mut self, at: usize, park: impl FnOnce() -> Q) -> TabId {
        let (tab, left) = self.leave_active(park);
        tab.place = Some(left);
        let id = self.fresh_id();
        self.list.insert(at.min(self.list.len()), Tab::new(id));
        self.active = id;
        self.mru.insert(0, id);
        id
    }

    /// The arriving tab's parked place; `None`, calling no `park`, if `id` is active or no tab.
    #[must_use]
    pub fn activate(&mut self, id: TabId, park: impl FnOnce() -> Q) -> Option<Q> {
        if id == self.active {
            return None;
        }
        let index = self.index_of(id)?;
        let (tab, left) = self.leave_active(park);
        tab.place = Some(left);
        self.active = id;
        self.mru.retain(|&other| other != id);
        self.mru.insert(0, id);
        self.list[index].place.take()
    }

    // The one caller of `park` and `capture`: they may have side effects, so each runs once per
    // real leave.
    fn leave_active<R>(&mut self, leave: impl FnOnce() -> R) -> (&mut Tab<P, Q>, R) {
        let index = self.active_index();
        (&mut self.list[index], leave())
    }

    fn fresh_id(&mut self) -> TabId {
        let id = TabId(self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests;
