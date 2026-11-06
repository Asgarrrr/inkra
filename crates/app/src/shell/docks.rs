//! The two docks: the view each draws under its header, and the toggles that show and hide them (docks spec).

use std::cmp::Ordering;
use std::collections::HashSet;

use gpui::{
    Action, AnyElement, App, AppContext, Context, Div, Focusable, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, actions, div, prelude::FluentBuilder,
};
use inkra_layout::{GroupId, Side, View};
use inkra_tabs::{Closed, TabId};

use super::dock_drag::DraggedView;
use super::group::Group;
use super::parked::Parked;
use super::tabs::{Place, capture};
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::sidebar::Sidebar;
use crate::theme;
use crate::ui::{ContextMenu, IconButton, IconName, PanelHeader, Tooltip, keep_focus};

actions!(
    docks,
    [
        ToggleLeftDock,
        ToggleRightDock,
        MoveFilesToOtherDock,
        MoveBacklinksToOtherDock
    ]
);

/// Each dock's views in tab order, active view and flag, left then right.
pub(super) type DockState = [(Vec<View>, Option<View>, bool); 2];

pub(super) fn title(view: View) -> &'static str {
    match view {
        View::Files => "Files",
        View::Backlinks => "Backlinks",
    }
}

pub(super) fn icon(view: View) -> IconName {
    match view {
        View::Files => IconName::Files,
        View::Backlinks => IconName::Link,
    }
}

fn icon_id(view: View) -> &'static str {
    match view {
        View::Files => "dock-icon-files",
        View::Backlinks => "dock-icon-backlinks",
    }
}

fn bar_selector(side: Side) -> &'static str {
    match side {
        Side::Left => "dock-bar-left",
        Side::Right => "dock-bar-right",
    }
}

fn header_selector(view: View) -> &'static str {
    match view {
        View::Files => "panel-header-files",
        View::Backlinks => "panel-header-backlinks",
    }
}

fn column_selector(side: Side) -> &'static str {
    match side {
        Side::Left => "dock-left",
        Side::Right => "dock-right",
    }
}

fn width(side: Side) -> Pixels {
    match side {
        Side::Left => LEFT_DOCK_WIDTH,
        Side::Right => RIGHT_DOCK_WIDTH,
    }
}

/// A dock's column: fixed width, the body's full height.
fn side_column(selector: &'static str, width: Pixels) -> Div {
    div()
        .debug_selector(move || selector.into())
        .flex_none()
        .w(width)
        .h_full()
}

impl AppShell {
    pub(super) fn toggle_left_dock(
        &mut self,
        _: &ToggleLeftDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_dock(Side::Left, window, cx);
    }

    pub(super) fn toggle_right_dock(
        &mut self,
        _: &ToggleRightDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_dock(Side::Right, window, cx);
    }

    pub(super) fn move_files_to_other_dock(
        &mut self,
        _: &MoveFilesToOtherDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_other_dock(View::Files, window, cx);
    }

    pub(super) fn move_backlinks_to_other_dock(
        &mut self,
        _: &MoveBacklinksToOtherDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_other_dock(View::Backlinks, window, cx);
    }

    fn move_to_other_dock(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.side_of(view).other();
        self.append(view, side, window, cx);
    }

    /// `view`'s menu: move it to the other dock, or hide its own.
    fn open_dock_menu(
        &mut self,
        view: View,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes.is_none() {
            return;
        }
        let side = self.side_of(view);
        let move_label = match side.other() {
            Side::Left => "Move to Left Dock",
            Side::Right => "Move to Right Dock",
        };
        let move_action: Box<dyn Action> = match view {
            View::Files => Box::new(MoveFilesToOtherDock),
            View::Backlinks => Box::new(MoveBacklinksToOtherDock),
        };
        let hide_action: Box<dyn Action> = match side {
            Side::Left => Box::new(ToggleLeftDock),
            Side::Right => Box::new(ToggleRightDock),
        };
        let entries = vec![
            (SharedString::from(move_label), move_action),
            (SharedString::from("Hide Dock"), hide_action),
        ];
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
    }

    pub(super) fn toggle_dock(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, _, _| shell.layout.toggle(side));
    }

    /// Activates `view` in its dock and shows that dock.
    pub(super) fn reveal(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.change_layout(window, cx, |shell, window, cx| {
            let (side, group, tab) = shell.tab_of(view);
            shell.activate_tab(group, tab, window, cx);
            shell.layout.show(side);
        });
    }

    /// Docks rules 1 and 2. `index` counts in `side`'s dock after `view` left it.
    ///
    /// # Panics
    /// If `index` is past the end of that dock.
    pub(super) fn move_view(
        &mut self,
        view: View,
        side: Side,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_layout(window, cx, |shell, window, cx| {
            let (from, source, tab) = shell.tab_of(view);
            let len = shell.dock_views(side).len() - usize::from(from == side);
            assert!(
                index <= len,
                "index {index} is past the end of a dock of {len}"
            );
            if from == side {
                shell.group_mut(source).tabs.move_tab(tab, index);
                shell.activate_tab(source, tab, window, cx);
            } else {
                shell.take_view_tab(source, tab, window, cx);
                shell.put_view_tab(view, side, index, window, cx);
            }
            shell.layout.show(side);
        });
    }

    /// Moves `view` to the end of `side`'s dock, as `move_view` does.
    pub(super) fn append(
        &mut self,
        view: View,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.dock_views(side).len() - usize::from(self.side_of(view) == side);
        self.move_view(view, side, index, window, cx);
    }

    /// A dock's last tab takes its group with it (docks rule 3).
    fn take_view_tab(
        &mut self,
        group: GroupId,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group(group).tabs.count() == 1 {
            self.layout.remove(group);
            self.groups.remove(&group);
            return;
        }
        let Group { tabs, shown, .. } = self.group_mut(group);
        let closed = tabs.close(tab, |slot| match slot {
            None => capture(shown, cx),
            Some(parked) => parked.unpark(cx),
        });
        if let Some(Closed {
            shown: Some((_, parked)),
            ..
        }) = closed
        {
            let place = parked.unpark(cx);
            self.display(group, place, window, cx);
        }
    }

    fn put_view_tab(
        &mut self,
        view: View,
        side: Side,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self.layout.dock(side) else {
            let group = self.layout.add_dock(side);
            self.groups.insert(group, Group::holding(&[view], 0));
            return;
        };
        let Group { tabs, shown, .. } = self.group_mut(group);
        tabs.insert(index, || Parked::park(shown, cx));
        self.display(group, Place::View(view), window, cx);
    }

    /// Its dock is drawn and it is that dock's active view.
    pub(super) fn is_visible(&self, view: View) -> bool {
        let (side, group, _) = self.tab_of(view);
        self.layout.is_drawn(side) && self.group(group).shown.view() == Some(view)
    }

    pub(super) fn side_of(&self, view: View) -> Side {
        self.tab_of(view).0
    }

    /// In tab order; empty for a dock with no group.
    pub(super) fn dock_views(&self, side: Side) -> Vec<View> {
        self.layout.dock(side).map_or_else(Vec::new, |group| {
            self.group(group).views().map(|(_, view)| view).collect()
        })
    }

    pub(super) fn dock_active(&self, side: Side) -> Option<View> {
        self.group(self.layout.dock(side)?).shown.view()
    }

    /// # Panics
    /// Only on a broken invariant: each view sits in exactly one dock tab.
    fn tab_of(&self, view: View) -> (Side, GroupId, TabId) {
        for side in [Side::Left, Side::Right] {
            let Some(group) = self.layout.dock(side) else {
                continue;
            };
            if let Some((tab, _)) = self.group(group).views().find(|&(_, held)| held == view) {
                return (side, group, tab);
            }
        }
        panic!("{view:?} sits in no dock");
    }

    pub(super) fn dock_state(&self) -> DockState {
        [Side::Left, Side::Right].map(|side| {
            (
                self.dock_views(side),
                self.dock_active(side),
                self.layout.is_shown(side),
            )
        })
    }

    /// The one writer of `layout` and the dock groups. A hidden tree keeps neither focus nor an open name field: either
    /// would take keys out of sight. An open menu closes: it describes the layout it opened on.
    pub(super) fn change_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) {
        if self.modal_menu().is_some() {
            self.dismiss_modal(window, cx);
        }
        if self.panes.is_none() {
            return;
        }
        let (layout, docks) = (self.layout.clone(), self.dock_state());
        let files_shown = self.is_visible(View::Files);
        change(self, window, cx);
        if self.layout == layout && self.dock_state() == docks {
            return;
        }
        if files_shown
            && !self.is_visible(View::Files)
            && let Some(panes) = &self.panes
        {
            let sidebar = panes.sidebar.clone();
            let focused = sidebar.focus_handle(cx).contains_focused(window, cx);
            sidebar.update(cx, Sidebar::cancel_edit);
            if focused && let Some(focus) = self.land_focus(cx) {
                window.focus(&focus, cx);
            }
        }
        if cfg!(debug_assertions) {
            self.check_layout(cx);
        }
        cx.notify();
        self.save_session_later(cx);
    }

    /// Debug builds check these after every layout change and the launch's.
    ///
    /// # Panics
    /// On a broken one.
    pub(super) fn check_layout(&self, cx: &App) {
        let held: HashSet<GroupId> = self.layout.groups().collect();
        assert_eq!(
            self.groups.keys().copied().collect::<HashSet<_>>(),
            held,
            "a group for each of the layout's, and no other"
        );
        let last = self.layout.last_center();
        assert!(
            self.layout.center().contains(&last),
            "the last active group is in the centre"
        );
        for &group in self.layout.center() {
            assert_eq!(
                self.group(group).views().count(),
                0,
                "the centre holds no view"
            );
            assert!(
                self.editor_of(group)
                    .is_none_or(|editor| editor.read(cx).is_pinned() == (group != last)),
                "an editor is pinned exactly when its group is not the last active"
            );
        }
        let mut views = Vec::new();
        for side in [Side::Left, Side::Right] {
            if let Some(group) = self.layout.dock(side) {
                let group = self.group(group);
                assert_eq!(
                    group.views().count(),
                    group.tabs.count(),
                    "a dock tab holds a view"
                );
                views.extend(group.views().map(|(_, view)| view));
            }
        }
        for view in View::ALL {
            let tabs = views.iter().filter(|&&held| held == view).count();
            assert_eq!(tabs, 1, "{view:?} sits in exactly one tab");
        }
    }

    /// `None` with no vault or an undrawn dock. A hidden view keeps its entity, so it comes back as it was left.
    pub(super) fn render_dock(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let panes = self.panes.as_ref()?;
        if !self.layout.is_drawn(side) {
            return None;
        }
        let group = self.group(self.layout.dock(side)?);
        let view = group.shown.view()?;
        let views: Vec<_> = group.views().map(|(_, view)| view).collect();
        let (body, count) = match view {
            View::Files => (panes.sidebar.clone().into_any_element(), None),
            View::Backlinks => (
                panes.backlinks.clone().into_any_element(),
                panes.backlinks.read(cx).count(),
            ),
        };
        let colors = theme::current(cx).colors;
        let column = side_column(column_selector(side), width(side))
            .bg(colors.background)
            .border_color(colors.border_variant)
            .flex()
            .flex_col()
            .drag_over::<DraggedView>(move |style, _, _, _| style.bg(colors.accent_background))
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    shell.append(dragged.view, side, window, cx);
                }),
            );
        let column = match side {
            Side::Left => column.border_r(theme::RULE_WIDTH),
            Side::Right => column.border_l(theme::RULE_WIDTH),
        };
        let header =
            match views.as_slice() {
                [_] => div()
                    .id(("dock-title-drag", usize::from(side == Side::Right)))
                    .on_drag(DraggedView { view, index: 0 }, |dragged, _, _, cx| {
                        cx.new(|_| dragged.clone())
                    })
                    .child(
                        PanelHeader::new(header_selector(view), title(view))
                            .when_some(count, PanelHeader::count)
                            .on_mouse_down(MouseButton::Right, Self::open_menu_on_press(view, cx))
                            .on_mouse_down(MouseButton::Left, keep_focus),
                    )
                    .into_any_element(),
                views => PanelHeader::bar(bar_selector(side))
                    .children(views.iter().enumerate().map(|(ix, &held)| {
                        Self::render_dock_icon(held, held == view, side, ix, cx)
                    }))
                    .on_mouse_down(MouseButton::Left, keep_focus)
                    .into_any_element(),
            };
        Some(
            column
                .child(header)
                .child(div().flex_1().min_h_0().child(body)),
        )
    }

    /// Rule 4: a click on the active icon changes nothing, as `change_layout` drops an equal layout.
    /// A drop on it inserts at `ix`, counted after the dragged view left (`Tabs::move_tab`'s rule).
    fn render_dock_icon(
        view: View,
        active: bool,
        side: Side,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::current(cx).colors;
        IconButton::new(icon_id(view), icon(view), title(view))
            .toggled(active)
            .tooltip(Tooltip::text(title(view)))
            .on_mouse_down(MouseButton::Right, Self::open_menu_on_press(view, cx))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.reveal(view, window, cx);
            }))
            .on_drag(DraggedView { view, index: ix }, |dragged, _, _, cx| {
                cx.new(|_| dragged.clone())
            })
            .drag_over::<DraggedView>(move |style, dragged, _, _| {
                let style = style
                    .bg(colors.accent_background)
                    .border_color(colors.accent);
                match ix.cmp(&dragged.index) {
                    Ordering::Less => style.border_l(theme::DROP_EDGE),
                    Ordering::Greater => style.border_r(theme::DROP_EDGE),
                    Ordering::Equal => style,
                }
            })
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    shell.move_view(dragged.view, side, ix, window, cx);
                }),
            )
            .into_any_element()
    }

    /// `prevent_default`: the root would take focus from the menu on the press (`div.rs:2765-2780`).
    fn open_menu_on_press(
        view: View,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
            window.prevent_default();
            shell.open_dock_menu(view, event.position, window, cx);
        })
    }

    /// `None` with no vault or an empty dock: there is no dock to toggle (rule 3).
    pub(super) fn render_dock_toggles(
        &self,
        cx: &mut Context<Self>,
    ) -> (
        Option<impl IntoElement + use<>>,
        Option<impl IntoElement + use<>>,
    ) {
        let has_views = |side| self.panes.is_some() && self.layout.dock(side).is_some();
        let left = has_views(Side::Left).then(|| {
            IconButton::new("dock-toggle-left", IconName::PanelLeft, "Toggle left dock")
                .toggled(self.layout.is_shown(Side::Left))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.toggle_left_dock(&ToggleLeftDock, window, cx);
                }))
        });
        let right = has_views(Side::Right).then(|| {
            IconButton::new(
                "dock-toggle-right",
                IconName::PanelRight,
                "Toggle right dock",
            )
            .toggled(self.layout.is_shown(Side::Right))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_right_dock(&ToggleRightDock, window, cx);
            }))
        });
        (left, right)
    }
}
