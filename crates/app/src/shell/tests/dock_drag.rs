//! Dragging a dock view: onto an icon, onto a dock column, onto nothing (docks spec, Drag).

use gpui::{Modifiers, MouseButton, TestAppContext, point, px};
use inkra_layout::{Docks, Side, View};

use super::dock_menu::docks;
use super::dragging::drag;
use super::{ab_focused, editor_focused, open};
use crate::shell::docks::{MoveBacklinksToOtherDock, MoveFilesToOtherDock};

#[gpui::test]
fn dropping_an_icon_on_another_reorders_the_bar(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();

    drag(&mut cx, "dock-icon-backlinks", "dock-icon-files");

    let left = docks(&window, &mut cx).dock(Side::Left).clone();
    assert_eq!(left.views(), [View::Backlinks, View::Files]);
    assert_eq!(left.active(), Some(View::Backlinks));
}

#[gpui::test]
fn dropping_on_a_one_view_header_appends(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "panel-header-files", "panel-header-backlinks");

    let right = docks(&window, &mut cx).dock(Side::Right).clone();
    assert_eq!(right.views(), [View::Backlinks, View::Files]);
    assert_eq!(right.active(), Some(View::Files));
    assert!(cx.debug_bounds("dock-left").is_none());
}

#[gpui::test]
fn dropping_on_a_dock_body_appends(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "panel-header-backlinks", "dock-left");

    let left = docks(&window, &mut cx).dock(Side::Left).clone();
    assert_eq!(left.views(), [View::Files, View::Backlinks]);
    assert_eq!(left.active(), Some(View::Backlinks));
}

#[gpui::test]
fn a_drop_outside_any_target_changes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "panel-header-files", "note-column");

    assert_eq!(docks(&window, &mut cx), Docks::default());
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn an_emptied_dock_is_refilled_by_a_drop_on_its_strip(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveFilesToOtherDock);
    cx.run_until_parked();
    assert!(cx.debug_bounds("dock-drop-left").is_none());

    let from = cx.debug_bounds("dock-icon-files").expect("drawn").center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(4.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("dock-drop-right").is_none());
    let strip = cx.debug_bounds("dock-drop-left").expect("drawn").center();
    cx.simulate_mouse_move(strip, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(strip, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let left = docks(&window, &mut cx).dock(Side::Left).clone();
    assert_eq!(left.views(), [View::Files]);
    assert_eq!(left.active(), Some(View::Files));
    assert!(left.is_drawn());
    assert!(cx.debug_bounds("dock-drop-left").is_none());
}
