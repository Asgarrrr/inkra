//! The `⌘K` keys: a split in each direction, and focus to the neighbour (splits spec, P3).

use gpui::{App, Context, FocusHandle, Focusable, Window, actions};
use inkra_layout::{Axis, GroupId, SplitDirection, Surface, View};

use super::docks::SplitView;
use super::group::Group;
use super::tabs::{Place, capture};
use super::{AppShell, Shown};

actions!(
    pane,
    [
        SplitRight,
        SplitDown,
        SplitLeft,
        SplitUp,
        ActivatePaneLeft,
        ActivatePaneRight,
        ActivatePaneUp,
        ActivatePaneDown
    ]
);

impl AppShell {
    pub(super) fn split_right(
        &mut self,
        _: &SplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_in(
            self.active_group(window, cx),
            SplitDirection::Right,
            window,
            cx,
        );
    }

    pub(super) fn split_down(
        &mut self,
        _: &SplitDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_in(
            self.active_group(window, cx),
            SplitDirection::Down,
            window,
            cx,
        );
    }

    pub(super) fn split_left(
        &mut self,
        _: &SplitLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_in(
            self.active_group(window, cx),
            SplitDirection::Left,
            window,
            cx,
        );
    }

    pub(super) fn split_up(&mut self, _: &SplitUp, window: &mut Window, cx: &mut Context<Self>) {
        self.split_in(
            self.active_group(window, cx),
            SplitDirection::Up,
            window,
            cx,
        );
    }

    pub(super) fn activate_pane_left(
        &mut self,
        _: &ActivatePaneLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_pane(SplitDirection::Left, window, cx);
    }

    pub(super) fn activate_pane_right(
        &mut self,
        _: &ActivatePaneRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_pane(SplitDirection::Right, window, cx);
    }

    pub(super) fn activate_pane_up(
        &mut self,
        _: &ActivatePaneUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_pane(SplitDirection::Up, window, cx);
    }

    pub(super) fn activate_pane_down(
        &mut self,
        _: &ActivatePaneDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_pane(SplitDirection::Down, window, cx);
    }

    /// The Files dock group while the tree holds focus, else `last_center`: no field stores it.
    fn active_group(&self, window: &Window, cx: &App) -> GroupId {
        if self
            .tree_focus(cx)
            .is_some_and(|tree| tree.contains_focused(window, cx))
        {
            self.tab_of(View::Files).0
        } else {
            self.layout.last_center()
        }
    }

    pub(super) fn tree_focus(&self, cx: &App) -> Option<FocusHandle> {
        let panes = self.panes.as_ref()?;
        Some(panes.sidebar.read(cx).focus_handle(cx))
    }

    /// A view moves out as its menu splits it: off a group it shares, with room for both. A
    /// centre group's note shows in both halves.
    fn split_in(
        &mut self,
        group: GroupId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.group(group).shown.view() {
            self.split_view(&SplitView { view, direction }, window, cx);
        } else if self.layout.surface_of(group) == Surface::Center {
            let shown = &self.group(group).shown;
            let place = match shown {
                Shown::Bound(_) | Shown::Image(_) => capture(shown, cx),
                Shown::Empty | Shown::Trashing(_) | Shown::View(_) => Place::Empty,
            };
            self.split_group(group, direction, place, window, cx);
        }
    }

    /// A fresh group on `from`'s `direction` side shows `place` and takes the focus; `None` where a
    /// half would fall below its minimum.
    pub(super) fn split_group(
        &mut self,
        from: GroupId,
        direction: SplitDirection,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<GroupId> {
        let mut fresh = None;
        self.change_layout(window, cx, |shell, window, cx| {
            let (room, mins) = (
                shell.room_along(from, direction.axis(), window),
                shell.min_sizes(window),
            );
            let Some(split) = shell
                .layout
                .split(from, direction, room, |group, axis| mins[&(group, axis)])
            else {
                return;
            };
            shell.groups.insert(split, Group::new());
            shell.display(split, place, window, cx);
            shell.switch_group(split, cx);
            fresh = Some(split);
        });
        if fresh.is_some()
            && let Some(focus) = self.land_focus(cx)
        {
            window.focus(&focus, cx);
        }
        fresh
    }

    /// Focus moves to the group past the active one's edge: a centre group's content, or the
    /// tree; Backlinks takes none.
    fn activate_pane(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from = self.active_group(window, cx);
        let probe = self.probe(from, direction, window, cx);
        let Some(to) = self.layout.neighbour(from, direction, probe) else {
            return;
        };
        let focus = match self.layout.surface_of(to) {
            Surface::Center => {
                self.activate_group(to, window, cx);
                self.land_focus(cx)
            }
            Surface::Dock(_) => (self.group(to).shown.view() == Some(View::Files))
                .then(|| self.tree_focus(cx))
                .flatten(),
        };
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
    }

    /// The caret's place along `from`'s edge in `direction`, as a fraction; half-way with no caret drawn.
    fn probe(&self, from: GroupId, direction: SplitDirection, window: &Window, cx: &App) -> f32 {
        let caret = self
            .editor_of(from)
            .and_then(|editor| editor.read(cx).caret_point());
        let (Some(caret), Some(bounds)) = (caret, self.bounds_of(from, window)) else {
            return 0.5;
        };
        let (at, start, length) = match direction.axis() {
            Axis::Horizontal => (caret.y, bounds.top(), bounds.size.height),
            Axis::Vertical => (caret.x, bounds.left(), bounds.size.width),
        };
        ((at - start) / length).clamp(0., 1.)
    }
}
