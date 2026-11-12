//! A tab leaving one group for another with its history, its place parked on the way, or splitting
//! a group by a drop near its edge (splits spec, P4).

use gpui::{App, Context, DragMoveEvent, Window};
use inkra_layout::{GroupId, Side, SplitDirection, Surface, View, drop_zone};
use inkra_tabs::{Detached, TabId, Tabs};

use super::dock_drag::DraggedView;
use super::docks::SplitView;
use super::group::Group;
use super::parked::Parked;
use super::tab_bar::DraggedTab;
use super::tabs::Place;
use super::{AppShell, Shown};

impl AppShell {
    /// Tab `id` of `from` to `to` at `at`, counted after it left (`Tabs::move_tab`'s rule). Within
    /// one group a reorder: the active tab stays (tabs spec, T4). Across groups it becomes `to`'s
    /// active tab and `to` comes forward: a centre group becomes the active one and takes the
    /// focus, a dock shows.
    ///
    /// # Panics
    /// If `at` is past the end of `to`.
    pub(super) fn move_tab_to(
        &mut self,
        from: GroupId,
        id: TabId,
        to: GroupId,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.holds((from, id)) {
            return;
        }
        let len = self.group(to).tabs.count() - usize::from(from == to);
        assert!(at <= len, "index {at} is past the end of a group of {len}");
        if from == to {
            self.move_tab(from, id, at, cx);
            return;
        }
        let surface = self.layout.surface_of(to);
        self.change_layout(window, cx, |shell, window, cx| {
            let detached = shell.detach(from, id, window, cx);
            shell.attach(detached, to, at, window, cx);
            match surface {
                Surface::Center => shell.switch_group(to, cx),
                Surface::Dock(side) => shell.layout.show(side),
            }
        });
        if surface == Surface::Center {
            self.land_focus(window, cx);
        }
    }

    /// `view`'s tab to `to` at `index`, as `move_tab_to` moves it (docks rules 1 and 2, P4).
    pub(super) fn move_view(
        &mut self,
        view: View,
        to: GroupId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (from, id) = self.tab_of(view);
        self.move_tab_to(from, id, to, index, window, cx);
    }

    /// Moves `view` to the end of `side`'s first group, as `move_view` does; to a fresh dock on an empty side.
    pub(super) fn append(
        &mut self,
        view: View,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(&group) = self.layout.dock_groups(side).first() {
            self.drop_view_at_end(view, group, window, cx);
            return;
        }
        self.change_layout(window, cx, |shell, window, cx| {
            let (source, tab) = shell.tab_of(view);
            let detached = shell.detach(source, tab, window, cx);
            let group = shell.layout.add_dock(side);
            shell.attach_alone(detached, group, window, cx);
            shell.layout.show(side);
        });
    }

    pub(super) fn split_view(
        &mut self,
        &SplitView { view, direction }: &SplitView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (group, tab) = self.tab_of(view);
        if self.group(group).tabs.count() >= 2 {
            self.split_off((group, tab), group, direction, window, cx);
        }
    }

    /// Whether tab `id` is still in `group`: a tab closed while dragged is not.
    fn holds(&self, (group, id): (GroupId, TabId)) -> bool {
        self.groups
            .get(&group)
            .is_some_and(|held| held.tabs.iter().any(|tab| tab.id() == id))
    }

    /// `view` to the end of `group`, counted after it left (`move_tab_to`'s rule).
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
            if self.layout.removable(from) {
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

    /// Whether a tab of `from` may split `group`: not `group`'s only tab, which would leave nothing behind.
    fn may_split(&self, group: GroupId, from: GroupId) -> bool {
        from != group || self.group(group).tabs.count() >= 2
    }

    /// `group`'s `drop_split` for a drag: none where `splits` is false, outside its body, or where
    /// a half would fall below its minimum.
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
            .filter(|direction| {
                // Built here, not up front: every group's listener sees every move of a drag.
                let mins = self.min_sizes(window);
                let room = self.room_along(group, direction.axis(), window);
                self.layout
                    .fits_split(group, direction.axis(), room, |group, axis| {
                        mins[&(group, axis)]
                    })
            });
        let held = &mut self.group_mut(group).drop_split;
        if *held != zone {
            *held = zone;
            cx.notify();
        }
    }

    pub(super) fn track_tab_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragMoveEvent<DraggedTab>, &mut Window, &mut App) + 'static {
        cx.listener(
            move |shell, event: &DragMoveEvent<DraggedTab>, window, cx| {
                let splits = shell.may_split(group, event.drag(cx).group);
                shell.track_drop(group, splits, event, window, cx);
            },
        )
    }

    /// As `track_tab_drop`, but a drop strip under the pointer takes the drop, so no edge splits.
    pub(super) fn track_view_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragMoveEvent<DraggedView>, &mut Window, &mut App) + 'static {
        cx.listener(
            move |shell, event: &DragMoveEvent<DraggedView>, window, cx| {
                let (from, _) = shell.tab_of(event.drag(cx).view);
                let splits = shell.may_split(group, from)
                    && !shell.drop_strip_hit()(event.event.position, window);
                shell.track_drop(group, splits, event, window, cx);
            },
        )
    }

    /// Tab `id` of `from` alone in a fresh group on `target`'s `direction` side, or nothing where
    /// that split does not fit. The split comes first, so `target` still exists when `from` is its
    /// sibling and the detach removes it. A fresh centre group becomes the active one and takes
    /// the focus.
    pub(super) fn split_off(
        &mut self,
        (from, id): (GroupId, TabId),
        target: GroupId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.holds((from, id)) {
            return;
        }
        let (room, mins) = (
            self.room_along(target, direction.axis(), window),
            self.min_sizes(window),
        );
        let min = |group, axis| mins[&(group, axis)];
        if !self.layout.fits_split(target, direction.axis(), room, min) {
            return;
        }
        let centre = self.layout.surface_of(target) == Surface::Center;
        self.change_layout(window, cx, |shell, window, cx| {
            let Some(fresh) = shell.layout.split(target, direction, room, min) else {
                panic!("{target:?} fits the split");
            };
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
        if centre {
            self.land_focus(window, cx);
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

    /// A view dropped on `group`'s body: a split where `drop_split` holds an edge, else the last
    /// tab, if it comes from another group.
    pub(super) fn view_body_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DraggedView, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, dragged: &DraggedView, window, cx| {
            if let Some(direction) = shell.group_mut(group).drop_split.take() {
                let tab = shell.tab_of(dragged.view);
                shell.split_off(tab, group, direction, window, cx);
            } else if dragged.group != group {
                shell.drop_view_at_end(dragged.view, group, window, cx);
            }
        })
    }
}
