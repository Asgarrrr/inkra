//! Dragging a dock view: onto an icon, onto a dock column, onto nothing (docks spec, Drag).

use std::cmp::Ordering;

use gpui::{Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px};
use inkra_layout::{Layout, Side, View};

use super::dock_menu::{default_docks, docks};
use super::dragging::drag;
use super::edge_drops::hold;
use super::splits::{centre, check_layout, close};
use super::{ab_focused, editor_focused, open};
use crate::shell::dock_drag::DraggedView;
use crate::shell::docks::{MoveBacklinksToOtherDock, MoveFilesToOtherDock};

#[test]
fn a_view_lands_beside_its_own_slot_and_before_a_foreign_one() {
    let layout = Layout::default();
    let (own, other) = (layout.dock_groups(Side::Left)[0], layout.last_center());
    let dragged = DraggedView {
        view: View::Files,
        index: 1,
        group: own,
    };

    assert_eq!(dragged.landing(own, 0), Ordering::Less);
    assert_eq!(dragged.landing(own, 1), Ordering::Equal);
    assert_eq!(dragged.landing(own, 2), Ordering::Greater);
    for ix in 0..3 {
        assert_eq!(dragged.landing(other, ix), Ordering::Less);
    }
}

#[gpui::test]
fn dropping_an_icon_on_another_reorders_the_bar(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();

    drag(&mut cx, "dock-icon-backlinks", "dock-icon-files");

    let [(views, active, _), _] = docks(&window, &mut cx);
    assert_eq!(views, [View::Backlinks, View::Files]);
    assert_eq!(active, Some(View::Backlinks));
}

#[gpui::test]
fn dropping_on_a_lone_icon_takes_its_place(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "dock-icon-files", "dock-icon-backlinks");

    let [_, (views, active, _)] = docks(&window, &mut cx);
    assert_eq!(views, [View::Files, View::Backlinks]);
    assert_eq!(active, Some(View::Files));
    assert!(cx.debug_bounds("dock-left").is_none());
}

#[gpui::test]
fn dropping_on_a_dock_body_appends(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "dock-icon-backlinks", "dock-left");

    let [(views, active, _), _] = docks(&window, &mut cx);
    assert_eq!(views, [View::Files, View::Backlinks]);
    assert_eq!(active, Some(View::Backlinks));
}

#[gpui::test]
fn a_drop_outside_any_target_changes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    drag(&mut cx, "dock-icon-files", "title-drag-left");

    assert_eq!(docks(&window, &mut cx), default_docks());
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

    let [(views, active, shown), _] = docks(&window, &mut cx);
    assert_eq!(views, [View::Files]);
    assert_eq!(active, Some(View::Files));
    assert!(shown);
    assert!(cx.debug_bounds("dock-drop-left").is_none());
}

/// Backlinks moved left, so the right strip lies over the note column's right edge zone.
fn right_strip_over_the_note(cx: &mut VisualTestContext) -> Point<Pixels> {
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();
    let column = cx.debug_bounds("note-column").expect("drawn");
    point(column.right() - px(10.), column.center().y)
}

#[gpui::test]
fn a_view_held_in_the_strip_shows_no_split(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let at = right_strip_over_the_note(&mut cx);

    hold(&mut cx, "dock-icon-backlinks", at);

    let strip = cx.debug_bounds("dock-drop-right").expect("drawn");
    assert!(strip.contains(&at), "{at:?} outside {strip:?}");
    let group = centre(&window, &mut cx)[0];
    let zone = window
        .update(&mut cx, |shell, _, _| shell.group(group).drop_split)
        .unwrap();
    assert_eq!(zone, None);
    assert!(cx.debug_bounds("drop-overlay").is_none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    close(&mut cx);
}

#[gpui::test]
fn a_view_dropped_in_the_strip_fills_the_dock_and_splits_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let at = right_strip_over_the_note(&mut cx);

    hold(&mut cx, "dock-icon-backlinks", at);
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let [_, (views, _, shown)] = docks(&window, &mut cx);
    assert_eq!(views, [View::Backlinks]);
    assert!(shown);
    assert_eq!(centre(&window, &mut cx).len(), 1);
    check_layout(&window, &mut cx);
    close(&mut cx);
}
