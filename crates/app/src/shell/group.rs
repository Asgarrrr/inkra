//! One tab group: its tabs, what its active tab shows and its pending open (splits spec, App).

use gpui::{ScrollHandle, Task};
use inkra_layout::View;
use inkra_tabs::{TabId, Tabs};

use super::Shown;
use super::parked::Parked;
use super::tabs::Place;

pub(super) struct Group {
    pub(super) tabs: Tabs<Place, Parked>,
    /// Written only by `set_shown`. Not `NoteStore::current`, which `trash` clears first.
    pub(super) shown: Shown,
    /// The bar's horizontal scroll, moved to the active tab on every switch.
    pub(super) tab_scroll: ScrollHandle,
    /// Held, never detached: dropping a `Task` cancels it, so a superseded
    /// load cannot rebind the editor behind a later activation.
    pub(super) activation: Option<Task<()>>,
}

impl Group {
    /// One empty tab.
    pub(super) fn new() -> Self {
        Self::with(Tabs::new(), Shown::Empty)
    }

    /// A tab on each of `views`, `active` the one shown.
    ///
    /// # Panics
    /// As `Tabs::restored`: on no views, or `active` out of range.
    pub(super) fn holding(views: &[View], active: usize) -> Self {
        let tabs = Tabs::restored(views.iter().copied().map(Parked::View).collect(), active);
        Self::with(tabs, Shown::View(views[active]))
    }

    fn with(tabs: Tabs<Place, Parked>, shown: Shown) -> Self {
        Self {
            tabs,
            shown,
            tab_scroll: ScrollHandle::new(),
            activation: None,
        }
    }

    /// The view each tab holds, in tab order; a tab holding none is skipped.
    pub(super) fn views(&self) -> impl Iterator<Item = (TabId, View)> + '_ {
        self.tabs.iter().filter_map(|tab| {
            let view = tab
                .place()
                .map_or_else(|| self.shown.view(), Parked::view)?;
            Some((tab.id(), view))
        })
    }
}
