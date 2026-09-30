use super::{Closed, Fake, TabId, Tabs, resolved};

/// Tabs of `Fake` whose `leaving` logs the tab left and returns the note showing with a new `seq`.
pub(super) struct Harness {
    pub(super) tabs: Tabs<Fake>,
    /// The active tab's place, which `Tabs` does not hold (H8): the caller's screen.
    pub(super) showing: Fake,
    pub(super) seq: u32,
    pub(super) left: Vec<TabId>,
}

impl Harness {
    pub(super) fn new() -> Self {
        Self {
            tabs: Tabs::new(),
            showing: Fake::default(),
            seq: 0,
            left: Vec::new(),
        }
    }

    pub(super) fn fresh(&mut self, note: Option<u8>) -> Fake {
        self.seq += 1;
        Fake {
            note,
            seq: self.seq,
        }
    }

    /// Runs `op` with a logging `leaving`, and returns the place it gave, if it was called.
    pub(super) fn with_leaving<R>(
        &mut self,
        op: impl FnOnce(&mut Tabs<Fake>, &mut dyn FnMut() -> Fake) -> R,
    ) -> (R, Option<Fake>) {
        let active = self.tabs.active();
        let note = self.showing.note;
        let mut given = None;
        let mut seq = self.seq;
        let left = &mut self.left;
        let result = op(&mut self.tabs, &mut || {
            seq += 1;
            left.push(active);
            let place = Fake { note, seq };
            given = Some(place.clone());
            place
        });
        self.seq = seq;
        (result, given)
    }

    pub(super) fn insert(&mut self, at: usize) -> TabId {
        let id = self
            .with_leaving(|tabs, leaving| tabs.insert(at, leaving))
            .0;
        self.showing = self.fresh(None);
        id
    }

    pub(super) fn activate(&mut self, id: TabId) -> Option<Fake> {
        let arrived = self
            .with_leaving(|tabs, leaving| tabs.activate(id, leaving))
            .0;
        if let Some(place) = &arrived {
            self.showing = place.clone();
        }
        arrived
    }

    pub(super) fn close(&mut self, id: TabId) -> Option<Closed<Fake>> {
        let closed = self
            .with_leaving(|tabs, leaving| tabs.close(id, leaving, resolved))
            .0;
        if let Some(Closed {
            shown: Some((_, place)),
            ..
        }) = &closed
        {
            self.showing = place.clone();
        }
        closed
    }

    pub(super) fn commit(&mut self, id: TabId, note: Option<u8>) -> bool {
        let committed = self
            .with_leaving(|tabs, leaving| tabs.commit(id, leaving))
            .0;
        if committed {
            self.showing = self.fresh(note);
        }
        committed
    }

    /// `back` or `forward` from the note showing; the arrival and the place left, if any.
    pub(super) fn walk(&mut self, back: bool) -> (Option<Fake>, Option<Fake>) {
        let showing = self.showing.note;
        let (arrived, left) = self.with_leaving(|tabs, leaving| {
            if back {
                tabs.back(showing, leaving)
            } else {
                tabs.forward(showing, leaving)
            }
        });
        if let Some(place) = &arrived {
            self.showing = place.clone();
        }
        (arrived, left)
    }

    pub(super) fn reopen(&mut self) -> Option<Fake> {
        let arrived = self.with_leaving(|tabs, leaving| tabs.reopen(leaving)).0;
        if let Some(place) = &arrived {
            self.showing = place.clone();
        }
        arrived
    }

    pub(super) fn forget(&mut self, note: u8) {
        self.tabs.forget(note);
        if self.showing.note == Some(note) {
            self.showing = Fake::default();
        }
    }

    pub(super) fn ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(crate::Tab::id).collect()
    }

    pub(super) fn places(&self) -> Vec<Option<Fake>> {
        self.tabs.iter().map(|tab| tab.place().cloned()).collect()
    }
}
