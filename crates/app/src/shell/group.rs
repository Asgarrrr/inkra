//! One tab group: its tabs, what its active tab shows and its pending open; the splits and activation (splits spec, App and P3).

use std::fmt::Display;

use gpui::{App, Context, FocusHandle, Focusable, ScrollHandle, Task, Window, actions};
use inkra_layout::{Axis, GroupId, SplitDirection, Surface, View};
use inkra_tabs::{TabId, Tabs};

use super::docks::SplitView;
use super::parked::Parked;
use super::tabs::{OnNote, Place, capture};
use super::{AppShell, Shown};
use crate::editor::EditorView;
use crate::theme;

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

/// The debug selector `name` takes in the centre's group `k`: bare in the first, so one group keeps every name.
pub(super) fn selector(name: impl Display, k: usize) -> String {
    if k == 0 {
        name.to_string()
    } else {
        format!("{name}-{k}")
    }
}

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

    /// One empty tab, no open pending, nothing to reopen.
    pub(super) fn holds_nothing(&self) -> bool {
        self.tabs.count() == 1
            && matches!(self.shown, Shown::Empty)
            && self.activation.is_none()
            && !self.tabs.has_closed()
    }

    pub(super) fn with(tabs: Tabs<Place, Parked>, shown: Shown) -> Self {
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

    /// A dock group splits as its view's menu does: off a group it shares, with room for both.
    fn split_in(
        &mut self,
        group: GroupId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.layout.surface_of(group) {
            Surface::Center => self.split(group, direction, window, cx),
            Surface::Dock(_) => {
                if let Some(view) = self.group(group).shown.view() {
                    self.split_view(&SplitView { view, direction }, window, cx);
                }
            }
        }
    }

    /// The new group shows the note with the caret it had, and takes the focus.
    fn split(
        &mut self,
        from: GroupId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_layout(window, cx, |shell, window, cx| {
            if !shell.fits_split(from, direction.axis(), window) {
                return;
            }
            let on = shell.group(from).shown.bound().map(|bound| OnNote {
                note: bound.note,
                view: Some(bound.editor.update(cx, |editor, cx| editor.capture(cx))),
            });
            let split = shell.layout.split(from, direction);
            shell.groups.insert(split, Group::new());
            if let Some(on) = on {
                shell.display(split, Place::Note(on), window, cx);
            }
            shell.switch_group(split, cx);
        });
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
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
        let (Some(caret), Some(rect)) = (caret, self.rect_of(from)) else {
            return 0.5;
        };
        let room = self.surface_size(Surface::Center, window);
        let (at, start, length) = match direction.axis() {
            Axis::Horizontal => (
                caret.y,
                theme::TITLE_BAR_HEIGHT + room.height * rect.y,
                room.height * rect.height,
            ),
            Axis::Vertical => (
                caret.x,
                self.dock_ends().0 + room.width * rect.x,
                room.width * rect.width,
            ),
        };
        ((at - start) / length).clamp(0., 1.)
    }

    /// Focus in the group left moves to the one entered: a pinned editor takes no keys.
    pub(super) fn activate_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let left = self.layout.last_center();
        if group == left {
            return;
        }
        let refocus = self.editor_of(left).is_some_and(|editor| {
            gpui::Focusable::focus_handle(editor, cx).contains_focused(window, cx)
        });
        self.change_layout(window, cx, |shell, _, cx| shell.switch_group(group, cx));
        if refocus && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// Closes `group`'s one tab `id` and removes the group; the heir takes its room and its closed
    /// stack, so `⌘⇧T` brings the tab back there (L6). Its pending open drops with it (T4).
    pub(super) fn close_group(
        &mut self,
        group: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert_eq!(self.layout.surface_of(group), Surface::Center);
        let was_last = group == self.layout.last_center();
        self.change_layout(window, cx, |shell, window, cx| {
            let Group { tabs, shown, .. } = shell.group_mut(group);
            tabs.close(id, |_| capture(shown, cx));
            shell.remove_group(group, window, cx);
        });
        if was_last && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// Takes `group` out of the layout; its heir takes its room, its closed stack and, if it was
    /// `last_center`, that place (L6). A dock's only group leaves no heir.
    pub(super) fn remove_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Group {
        let was_last = group == self.layout.last_center();
        if was_last {
            self.unfocus_editor(group, window, cx);
        } else if let Some(editor) = self.editor_of(group) {
            editor.update(cx, EditorView::unpin);
        }
        let heir = self.layout.remove(group);
        let Some(mut removed) = self.groups.remove(&group) else {
            panic!("{group:?} has no group");
        };
        if let Some(heir) = heir {
            self.group_mut(heir)
                .tabs
                .extend_closed(removed.tabs.take_closed());
            if was_last {
                self.enter_group(heir, cx);
            }
        }
        removed
    }

    pub(super) fn switch_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor() {
            editor.update(cx, EditorView::pin);
        }
        self.enter_group(group, cx);
    }

    /// `group` becomes `last_center`, unpinned, and what it shows reaches the note's followers.
    pub(super) fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_of(group) {
            editor.update(cx, EditorView::unpin);
        }
        self.layout.activate(group);
        let since = self.tree_cursor(cx);
        self.follow_center(since, cx);
        self.keep_active_tab_visible(group);
    }
}
