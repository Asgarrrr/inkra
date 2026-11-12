//! The history arrows and the history menu (obsidian-chrome spec, D4).

use gpui::{
    App, AppContext, Context, Div, ElementId, GestureTuning, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Styled, Task, Window,
};
use inkra_layout::GroupId;
use inkra_tabs::{Stop, Way};
use inkra_vault::EntryId;

use super::AppShell;
use super::group::{Group, selector};
use super::tab_bar::tab_title;
use super::tabs::capture;
use crate::theme;
use crate::ui::{ContextMenu, IconButton, IconName, MenuEntry};

/// The arrows' slot: its inset and two buttons; a centre group's minimum width counts it.
pub(super) fn nav_width() -> Pixels {
    theme::ROW_INSET + theme::ICON_BUTTON_WIDTH * 2.
}

/// The most stops the history menu lists each way.
const HISTORY_MENU_REACH: usize = 7;

/// A history menu row; the palette leaves it out, since it cannot be built without its stop.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = tabs, no_json)]
pub(crate) struct GoToStop {
    pub(crate) group: GroupId,
    pub(crate) stop: Stop<EntryId>,
}

/// A history walk: one step either way, or straight to a listed stop.
#[derive(Clone, Copy)]
pub(super) enum Walk {
    Step(Way),
    To(Stop<EntryId>),
}

/// One left press on an arrow.
pub(super) enum Press {
    /// Owns the hold timer, so dropping the press cancels the hold.
    Held { at: Point<Pixels>, _timer: Task<()> },
    /// The hold opened the menu: the click this press's release fires is swallowed.
    Opened,
}

impl AppShell {
    /// Before the tab bar, in `slot`, outside its scroll; each arrow is lit when its walk would arrive (D4).
    pub(super) fn render_nav(
        &self,
        k: usize,
        group: GroupId,
        slot: Div,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let tabs = &self.group(group).tabs;
        let showing = self.group(group).shown.file();
        slot.debug_selector(move || selector("nav", k))
            .pl(theme::ROW_INSET)
            .child(Self::render_arrow(
                k,
                group,
                Way::Back,
                tabs.can_go_back(showing),
                cx,
            ))
            .child(Self::render_arrow(
                k,
                group,
                Way::Forward,
                tabs.can_go_forward(showing),
                cx,
            ))
    }

    fn render_arrow(
        k: usize,
        group: GroupId,
        way: Way,
        lit: bool,
        cx: &mut Context<Self>,
    ) -> IconButton {
        let (id, icon, label) = match way {
            Way::Back => ("nav-back", IconName::ArrowLeft, "Go back"),
            Way::Forward => ("nav-forward", IconName::ArrowRight, "Go forward"),
        };
        IconButton::new(ElementId::Name(selector(id, k).into()), icon, label)
            .disabled(!lit)
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.click_arrow(group, way, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, Self::arm_on_press(group, cx))
            .on_mouse_up(MouseButton::Left, Self::release_on_up(cx))
            .on_mouse_up_out(MouseButton::Left, Self::release_on_up(cx))
            .on_mouse_down(MouseButton::Right, Self::history_menu_on_press(group, cx))
    }

    /// Lists `group`'s active tab's history at `at`, the current entry between the way forward
    /// and the way back; with no stop either way, opens nothing and returns false.
    pub(super) fn open_history_menu(
        &mut self,
        group: GroupId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let tabs = &self.group(group).tabs;
        let showing = self.group(group).shown.file();
        let forward: Vec<Stop<EntryId>> = tabs
            .stops(Way::Forward, showing)
            .take(HISTORY_MENU_REACH)
            .collect();
        let back: Vec<Stop<EntryId>> = tabs
            .stops(Way::Back, showing)
            .take(HISTORY_MENU_REACH)
            .collect();
        // `ContextMenu::new` panics on no entry, and a menu of the current row alone is no menu.
        if forward.is_empty() && back.is_empty() {
            return false;
        }
        let current = tab_title(showing, self.notes.read(cx), cx);
        self.activate_group(group, window, cx);
        self.land_focus(window, cx);
        let notes = self.notes.read(cx);
        let row = |stop: Stop<EntryId>, icon: IconName| {
            MenuEntry::new(
                tab_title(Some(stop.file), notes, cx),
                Box::new(GoToStop { group, stop }),
            )
            .icon(icon)
        };
        let entries: Vec<MenuEntry> = forward
            .into_iter()
            .rev()
            .map(|stop| row(stop, IconName::ArrowRight))
            .chain([MenuEntry::current(current)])
            .chain(back.into_iter().map(|stop| row(stop, IconName::ArrowLeft)))
            .collect();
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
        true
    }

    pub(super) fn go_to_stop(
        &mut self,
        action: &GoToStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.walk_history(action.group, Walk::To(action.stop), window, cx);
    }

    /// The one path every history walk takes: `group` names the tab walked, never the focus.
    pub(super) fn walk_history(
        &mut self,
        group: GroupId,
        walk: Walk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_modal(window, cx);
        let Some(Group { tabs, shown, .. }) = self.groups.get_mut(&group) else {
            return;
        };
        let note = shown.file();
        // Captured, not parked: the tab stays active, and the stack keeps plain offsets (A2).
        let leaving = || capture(shown, cx);
        let arriving = match walk {
            Walk::Step(Way::Back) => tabs.back(note, leaving),
            Walk::Step(Way::Forward) => tabs.forward(note, leaving),
            Walk::To(stop) => tabs.walk_to(stop, note, leaving),
        };
        if let Some(place) = arriving {
            self.show_place(group, place, window, cx);
        }
    }

    /// Starts the hold that opens `group`'s history menu; replaces any earlier press.
    fn arm_press(
        &mut self,
        group: GroupId,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let timer = cx.spawn_in(window, async move |shell, cx| {
            cx.background_executor()
                .timer(GestureTuning::default().long_press_duration)
                .await;
            let _ = shell.update_in(cx, |shell, window, cx| {
                shell.hold_elapsed(group, window, cx);
            });
        });
        self.press = Some(Press::Held {
            at: event.position,
            _timer: timer,
        });
    }

    fn hold_elapsed(&mut self, group: GroupId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Press::Held { at, .. }) = self.press.take() else {
            return;
        };
        if strayed(window.mouse_position(), at) || !self.groups.contains_key(&group) {
            return;
        }
        self.press = self
            .open_history_menu(group, at, window, cx)
            .then_some(Press::Opened);
    }

    /// Drops only a hold: an `Opened` press waits for its click to be swallowed.
    fn release_press(&mut self) {
        if matches!(self.press, Some(Press::Held { .. })) {
            self.press = None;
        }
    }

    fn click_arrow(
        &mut self,
        group: GroupId,
        way: Way,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.press.take(), Some(Press::Opened)) {
            self.walk_history(group, Walk::Step(way), window, cx);
        }
    }

    /// On the root: an arrow's own move listener runs only while it is hovered (`div.rs:303-313`).
    pub(super) fn press_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let Some(Press::Held { at, .. }) = &self.press else {
            return;
        };
        if strayed(event.position, *at) || event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
        }
    }

    fn arm_on_press(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
            shell.arm_press(group, event, window, cx);
        })
    }

    fn release_on_up(
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static {
        cx.listener(|shell, _: &MouseUpEvent, _, _| shell.release_press())
    }

    /// Opens `group`'s history menu at the press.
    fn history_menu_on_press(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
            // `prevent_default`: the root would take focus on the press (`div.rs:2765-2780`).
            window.prevent_default();
            shell.open_history_menu(group, event.position, window, cx);
        })
    }

    /// Where the back arrow was drawn last, for a scene to press on; from the first tab's edge, since the arrows sit before it.
    #[cfg(feature = "shot")]
    pub(crate) fn nav_back_center(&self) -> Option<Point<Pixels>> {
        let bounds = self.center().tab_scroll.bounds_for_item(0)?;
        Some(gpui::point(
            bounds.left() - theme::TAB_CURVE - nav_width()
                + theme::ROW_INSET
                + theme::ICON_BUTTON_WIDTH * 0.5,
            bounds.center().y,
        ))
    }
}

/// Past the slop, a press is a drag, not a hold.
fn strayed(now: Point<Pixels>, at: Point<Pixels>) -> bool {
    (now - at).magnitude() > f64::from(GestureTuning::default().touch_slop)
}
