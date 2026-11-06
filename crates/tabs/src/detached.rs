//! A tab between groups: it leaves one with its back and forward stacks and enters another (splits spec, P4).

use crate::history::Stack;
use crate::{Place, Tab, TabId, Tabs};

/// A tab out of every group, its place held as an inactive slot holds it.
#[derive(Debug, Clone)]
pub struct Detached<P, Q = P> {
    place: Q,
    back: Stack<P>,
    forward: Stack<P>,
}

/// What `take` returns: the tab, and the slot that now shows when the active tab left.
type Taken<P, Q> = (Detached<P, Q>, Option<(TabId, Q)>);

impl<P: Place + Clone, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// Tab `id` out of the group, with its slot, or what `leave` returns if it is active (H8); the
    /// MRU's next then takes over and its slot comes back to show. The closed stack is untouched.
    ///
    /// # Panics
    /// If `id` is the only tab: the caller removes the group instead.
    pub fn take(&mut self, id: TabId, leave: impl FnOnce() -> Q) -> Option<Taken<P, Q>> {
        let (_, tab) = self.remove(id)?;
        assert!(
            !self.list.is_empty(),
            "the only tab leaves with its group, never alone"
        );
        let Tab {
            place,
            back,
            forward,
            ..
        } = tab;
        let shown = place.is_none().then(|| self.show_next());
        let place = place.unwrap_or_else(leave);
        Some((
            Detached {
                place,
                back,
                forward,
            },
            shown,
        ))
    }

    /// The group's one tab, its place what `leave` returns.
    ///
    /// # Panics
    /// Unless the group holds exactly one tab.
    #[must_use]
    pub fn into_detached(mut self, leave: impl FnOnce() -> Q) -> Detached<P, Q> {
        assert_eq!(self.count(), 1, "only a group of one tab detaches whole");
        let tab = self.list.remove(0);
        Detached {
            place: leave(),
            back: tab.back,
            forward: tab.forward,
        }
    }

    /// Inserts `detached` at `at`, clamped, under a fresh id, and activates it as `insert` does;
    /// returns that id and the place to show.
    pub fn put(
        &mut self,
        detached: Detached<P, Q>,
        at: usize,
        park: impl FnOnce() -> Q,
    ) -> (TabId, Q) {
        let id = self.insert(at, park);
        self.set_active_history(detached.back, detached.forward);
        (id, detached.place)
    }

    /// A group of `detached` alone, active; the caller shows the place returned.
    #[must_use]
    pub fn detached(detached: Detached<P, Q>) -> (Self, Q) {
        let mut tabs = Self::new();
        tabs.set_active_history(detached.back, detached.forward);
        (tabs, detached.place)
    }
}
