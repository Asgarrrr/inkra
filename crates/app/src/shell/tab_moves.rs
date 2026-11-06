//! A tab leaving one group for another with its history, its place parked on the way, or splitting
//! a group by a drop near its edge (splits spec, P4).

use gpui::{App, Context, DragMoveEvent, Window};
use inkra_layout::{GroupId, SplitDirection, Surface, View, drop_zone};
use inkra_tabs::{Detached, TabId, Tabs};

use super::dock_drag::DraggedView;
use super::group::Group;
use super::parked::Parked;
use super::tab_bar::DraggedTab;
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

    /// `group`'s `drop_split` for the pointer: none outside its body, where a half would fall below
    /// its minimum, or unless `splits`.
    fn track_drop<T: 'static>(
        &mut self,
        group: GroupId,
        splits: bool,
        event: &DragMoveEvent<T>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (bounds, at) = (event.bounds, event.event.position);
        let zone = (splits && bounds.contains(&at))
            .then(|| {
                let at = at - bounds.origin;
                drop_zone(
                    (f32::from(bounds.size.width), f32::from(bounds.size.height)),
                    (f32::from(at.x), f32::from(at.y)),
                )
            })
            .flatten()
            .filter(|direction| self.fits_split(group, direction.axis(), window));
        let held = &mut self.group_mut(group).drop_split;
        if *held != zone {
            *held = zone;
            cx.notify();
        }
    }

    /// A group's only tab splits nothing of its own group: it would leave nothing behind.
    pub(super) fn track_tab_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragMoveEvent<DraggedTab>, &mut Window, &mut App) + 'static {
        cx.listener(
            move |shell, event: &DragMoveEvent<DraggedTab>, window, cx| {
                let splits = event.drag(cx).group != group || shell.group(group).tabs.count() >= 2;
                shell.track_drop(group, splits, event, window, cx);
            },
        )
    }

    pub(super) fn track_view_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragMoveEvent<DraggedView>, &mut Window, &mut App) + 'static {
        cx.listener(
            move |shell, event: &DragMoveEvent<DraggedView>, window, cx| {
                shell.track_drop(group, true, event, window, cx);
            },
        )
    }

    /// Tab `id` of `from` alone in a fresh group on `target`'s `direction` side. The split comes
    /// first, so `target` still exists when `from` is its sibling and the detach removes it. A
    /// fresh centre group becomes the active one and takes the focus.
    pub(super) fn split_off(
        &mut self,
        (from, id): (GroupId, TabId),
        target: GroupId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let centre = self.layout.surface_of(target) == Surface::Center;
        self.change_layout(window, cx, |shell, window, cx| {
            let fresh = shell.layout.split(target, direction);
            // The heir of a group the detach removes may be `fresh`, which takes its closed tabs.
            shell.groups.insert(fresh, Group::new());
            let detached = shell.detach(from, id, window, cx);
            let closed = shell.group_mut(fresh).tabs.take_closed();
            shell.attach_alone(detached, fresh, window, cx);
            shell.group_mut(fresh).tabs.extend_closed(closed);
            if centre {
                shell.switch_group(fresh, cx);
            }
        });
        if centre && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// A tab dropped on centre `group`'s body: a split where `drop_split` holds an edge, else the
    /// last tab, if it comes from another group.
    pub(super) fn tab_body_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DraggedTab, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, dragged: &DraggedTab, window, cx| {
            if let Some(direction) = shell.group_mut(group).drop_split.take() {
                shell.split_off((dragged.group, dragged.id), group, direction, window, cx);
            } else if dragged.group != group {
                let at = shell.group(group).tabs.count();
                shell.move_tab_to(dragged.group, dragged.id, group, at, window, cx);
            }
        })
    }

    /// A view dropped on `group`'s body: a split where `drop_split` holds an edge, else `land`.
    pub(super) fn view_body_drop(
        group: GroupId,
        land: impl Fn(&mut Self, View, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DraggedView, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, dragged: &DraggedView, window, cx| {
            match shell.group_mut(group).drop_split.take() {
                Some(direction) => {
                    let tab = shell.tab_of(dragged.view);
                    shell.split_off(tab, group, direction, window, cx);
                }
                None => land(shell, dragged.view, window, cx),
            }
        })
    }
}
