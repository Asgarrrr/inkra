//! A tab leaving one group for another with its history, its place parked on the way (splits spec, P4).

use gpui::{Context, Window};
use inkra_layout::{GroupId, View};
use inkra_tabs::{Detached, TabId, Tabs};

use super::group::Group;
use super::parked::Parked;
use super::tabs::Place;
use super::{AppShell, Shown};

impl AppShell {
    /// Note tabs move centre to centre only: `to` becomes the active group and takes the focus.
    pub(super) fn move_tab_to(
        &mut self,
        from: GroupId,
        id: TabId,
        to: GroupId,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            self.move_tab(from, id, at, cx);
            return;
        }
        self.change_layout(window, cx, |shell, window, cx| {
            let detached = shell.detach(from, id, window, cx);
            shell.attach(detached, to, at, window, cx);
            shell.switch_group(to, cx);
        });
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// `view` to the end of `group`, counted after it left (`move_view`'s rule).
    pub(super) fn drop_view_at_end(
        &mut self,
        view: View,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.group(group).tabs.count() - usize::from(self.tab_of(view).0 == group);
        self.move_view(view, group, index, window, cx);
    }

    /// Tab `id` out of `from`. A group of one tab goes with it (L6), but the centre keeps a group (L3).
    pub(super) fn detach(
        &mut self,
        from: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Detached<Place, Parked> {
        if self.group(from).tabs.count() == 1 {
            if self.layout.center_groups() != [from] {
                let place = Parked::park(&self.group(from).shown, cx);
                return self
                    .remove_group(from, window, cx)
                    .tabs
                    .into_detached(|| place);
            }
            let Group { tabs, shown, .. } = self.group_mut(from);
            tabs.insert(1, || Parked::park(shown, cx));
            self.display(from, Place::Empty, window, cx);
        }
        let Group { tabs, shown, .. } = self.group_mut(from);
        let Some((detached, next)) = tabs.take(id, || Parked::park(shown, cx)) else {
            panic!("{id:?} is not a tab of {from:?}");
        };
        if let Some((_, parked)) = next {
            let place = parked.unpark(cx);
            self.display(from, place, window, cx);
        }
        detached
    }

    /// `detached` into `to` at `at`, active, the tab it covers parked.
    pub(super) fn attach(
        &mut self,
        detached: Detached<Place, Parked>,
        to: GroupId,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Group { tabs, shown, .. } = self.group_mut(to);
        let (_, parked) = tabs.put(detached, at, || Parked::park(shown, cx));
        let place = parked.unpark(cx);
        self.display(to, place, window, cx);
        self.keep_active_tab_visible(to);
    }

    /// `detached` alone in fresh `group`, which the layout already holds.
    pub(super) fn attach_alone(
        &mut self,
        detached: Detached<Place, Parked>,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tabs, parked) = Tabs::detached(detached);
        self.groups.insert(group, Group::with(tabs, Shown::Empty));
        let place = parked.unpark(cx);
        self.display(group, place, window, cx);
    }
}
