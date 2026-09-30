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
pub trait Place: Default + Clone {
    type Note: Copy + Eq;
    /// `None`: an empty tab. Never pushed on a stack (invariant 5).
    fn note(&self) -> Option<Self::Note>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

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

impl<P: Place> Tabs<P> {
    /// Tabs on `places` in bar order; the caller shows `active`'s (H8). Fresh ids, MRU the active first.
    ///
    /// # Panics
    /// If `places` is empty or `active` is out of range.
    #[must_use]
    pub fn restored(places: Vec<P>, active: usize) -> Self {
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
