//! A tab or a view dropped near a group's edge splits the group there (splits spec, P4).

use gpui::{
    Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px, size,
};
use inkra_layout::{Side, View};

use super::dragging::drag_to;
use super::splits::{centre, check_layout, close, editor_in, focused, last, split, tabs_in};
use super::{ab_focused, keys, open};
use crate::shell::docks::MoveBacklinksToOtherDock;

/// A point at `x` of `selector`'s width, half-way down.
fn across(cx: &mut VisualTestContext, selector: &'static str, x: f32) -> Point<Pixels> {
    let bounds = cx.debug_bounds(selector).expect("drawn");
    point(bounds.left() + bounds.size.width * x, bounds.center().y)
}

/// `drag_to` short of the release: the drag stays held over `to`.
fn hold(cx: &mut VisualTestContext, from: &'static str, to: Point<Pixels>) {
    let from = cx.debug_bounds(from).expect("drawn").center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(4.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn dropping_a_tab_near_a_groups_right_edge_splits_right(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    let edge = across(&mut cx, "note-column", 0.95);

    drag_to(&mut cx, "tab-1", edge);

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b)]);
    assert_eq!(last(&window, &mut cx), groups[1]);
    let editor = editor_in(&window, &mut cx, 1);
    assert!(focused(&window, &mut cx, &editor));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn dropping_a_groups_only_tab_on_its_own_edge_does_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let edge = across(&mut cx, "note-column", 0.95);

    drag_to(&mut cx, "tab-0", edge);

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

/// `Layout::remove` names `fresh` the heir of the emptied group to its right.
#[gpui::test]
fn dropping_another_groups_only_tab_near_an_edge_splits_beside_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    let edge = across(&mut cx, "note-column", 0.95);

    drag_to(&mut cx, "tab-0-1", edge);

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(b)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b)]);
    assert_eq!(last(&window, &mut cx), groups[1]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn dropping_in_the_middle_moves_the_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    let middle = across(&mut cx, "note-column-1", 0.5);

    drag_to(&mut cx, "tab-0", middle);

    assert_eq!(centre(&window, &mut cx).len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(b)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b), Some(a)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn the_overlay_covers_the_right_half_near_the_right_edge(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    let column = cx.debug_bounds("note-column").expect("drawn");
    let edge = across(&mut cx, "note-column", 0.95);

    hold(&mut cx, "tab-1", edge);

    let overlay = cx.debug_bounds("drop-overlay").expect("drawn");
    assert!(
        (overlay.left() - column.center().x).abs() < px(1.)
            && (overlay.right() - column.right()).abs() < px(1.),
        "{overlay:?} over {column:?}"
    );
    cx.simulate_mouse_up(edge, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn an_edge_drop_that_does_not_fit_only_moves(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.run_until_parked();
    let edge = across(&mut cx, "note-column-1", 0.95);

    drag_to(&mut cx, "tab-0", edge);

    assert_eq!(centre(&window, &mut cx).len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b), Some(a)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_view_dropped_near_a_dock_groups_bottom_edge_splits_the_dock(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();
    let group = cx.debug_bounds("dock-left-group").expect("drawn");

    drag_to(
        &mut cx,
        "dock-icon-backlinks",
        point(group.center().x, group.bottom() - px(10.)),
    );

    let held = window
        .update(&mut cx, |shell, _, _| {
            let groups = shell.layout.dock_groups(Side::Left);
            groups
                .iter()
                .map(|&group| shell.group(group).views().map(|(_, view)| view).collect())
                .collect::<Vec<Vec<_>>>()
        })
        .unwrap();
    assert_eq!(held, [vec![View::Files], vec![View::Backlinks]]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}
