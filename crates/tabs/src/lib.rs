//! Tabs with stable ids, an MRU and the index rule, generic over the place a tab shows (tabs spec, A1).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

#[derive(Debug, Clone)]
pub struct Tab<P> {
    id: TabId,
    place: P,
}

impl<P> Tab<P> {
    #[must_use]
    pub fn id(&self) -> TabId {
        self.id
    }

    #[must_use]
    pub fn place(&self) -> &P {
        &self.place
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

#[derive(Debug)]
pub struct Tabs<P> {
    list: Vec<Tab<P>>,
    active: TabId,
    mru: Vec<TabId>,
    next: u64,
}

impl<P> Tabs<P> {
    #[must_use]
    pub fn new(first: P) -> Self {
        let id = TabId(0);
        Self {
            list: vec![Tab { id, place: first }],
            active: id,
            mru: vec![id],
            next: 1,
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
    pub fn active_place(&self) -> &P {
        &self.list[self.active_index()].place
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

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.list.iter().position(|tab| tab.id == id)
    }
}

impl<P: Clone + Default> Tabs<P> {
    /// Opens `place` in a new tab at `at`, clamped to the bar, and activates it.
    pub fn insert(&mut self, at: usize, place: P, leaving: impl FnOnce() -> P) -> TabId {
        self.leave_active(leaving);
        let id = self.fresh_id();
        self.list.insert(at.min(self.list.len()), Tab { id, place });
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
        self.leave_active(leaving);
        self.active = id;
        self.mru.retain(|&other| other != id);
        self.mru.insert(0, id);
        Some(self.list[index].place.clone())
    }

    /// The last tab gives way to an empty one with a fresh id: a load keyed by the old id lands nowhere.
    pub fn close(&mut self, id: TabId, leaving: impl FnOnce() -> P) -> Option<Closed<P>> {
        let index = self.index_of(id)?;
        let was_active = id == self.active;
        if was_active {
            self.leave_active(leaving);
        }
        let place = self.list.remove(index).place;
        self.mru.retain(|&other| other != id);
        if self.list.is_empty() {
            let fresh = self.fresh_id();
            self.list.push(Tab {
                id: fresh,
                place: P::default(),
            });
            self.mru.push(fresh);
        }
        let shown = was_active.then(|| {
            self.active = self.mru[0];
            (self.active, self.active_place().clone())
        });
        Some(Closed {
            place,
            index,
            shown,
        })
    }

    /// Replaces the place of tab `id` if it is still active; else changes nothing.
    #[must_use]
    pub fn commit(&mut self, id: TabId, place: P, leaving: impl FnOnce() -> P) -> bool {
        if id != self.active {
            return false;
        }
        self.leave_active(leaving);
        let index = self.active_index();
        self.list[index].place = place;
        true
    }

    /// Empties every tab whose place is `gone`; returns whether the active one was.
    #[must_use]
    pub fn forget(&mut self, gone: impl Fn(&P) -> bool) -> bool {
        let mut active = false;
        for tab in &mut self.list {
            if gone(&tab.place) {
                tab.place = P::default();
                active |= tab.id == self.active;
            }
        }
        active
    }

    // The one caller of `leaving`: it may have side effects, so it runs once per real leave.
    fn leave_active(&mut self, leaving: impl FnOnce() -> P) {
        let index = self.active_index();
        self.list[index].place = leaving();
    }

    fn fresh_id(&mut self) -> TabId {
        let id = TabId(self.next);
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests;
