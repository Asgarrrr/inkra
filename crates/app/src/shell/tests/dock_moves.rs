//! Moving and revealing a dock view through the shell's one writer (docks rules 1 and 2, splits spec S1).

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Side, ViewId};

use super::ab_focused;
use super::dock_menu::{Docks, docks};
use crate::backlinks::BACKLINKS;
use crate::shell::AppShell;
use crate::shell::FILES;

fn shell(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut AppShell, &mut gpui::Window, &mut gpui::Context<AppShell>),
) {
    window.update(cx, change).unwrap();
    cx.run_until_parked();
}

fn move_view(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    view: ViewId,
    side: Side,
    index: usize,
) {
    shell(window, cx, |shell, window, cx| {
        if let Some(&group) = shell.layout.dock_groups(side).first() {
            shell.move_view(view, group, index, window, cx);
        } else {
            assert_eq!(index, 0, "an empty side takes the view first");
            shell.append(view, side, window, cx);
        }
    });
}

fn append(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, view: ViewId, side: Side) {
    shell(window, cx, |shell, window, cx| {
        shell.append(view, side, window, cx);
    });
}

fn reveal(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, view: ViewId) {
    shell(window, cx, |shell, window, cx| {
        shell.reveal(view, window, cx);
    });
}

fn toggle(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, side: Side) {
    shell(window, cx, |shell, window, cx| {
        shell.toggle_dock(side, window, cx);
    });
}

/// Both views in the left dock, Files first, `active` shown; the right dock empty.
fn both_left(
    cx: &mut TestAppContext,
    active: ViewId,
) -> (WindowHandle<AppShell>, VisualTestContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    move_view(&window, &mut cx, BACKLINKS, Side::Left, 1);
    if active == FILES {
        reveal(&window, &mut cx, FILES);
    }
    assert_eq!(
        docks(&window, &mut cx),
        [
            (vec![FILES, BACKLINKS], Some(active), true),
            (Vec::new(), None, true),
        ]
    );
    (window, cx)
}

fn left(docks: Docks) -> (Vec<ViewId>, Option<ViewId>, bool) {
    let [left, _] = docks;
    left
}

#[gpui::test]
fn reveal_shows_a_hidden_dock_and_activates_the_view(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, BACKLINKS);
    toggle(&window, &mut cx, Side::Left);

    reveal(&window, &mut cx, FILES);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![FILES, BACKLINKS], Some(FILES), true)
    );
}

#[gpui::test]
fn reveal_leaves_the_other_dock_alone(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    toggle(&window, &mut cx, Side::Right);
    toggle(&window, &mut cx, Side::Left);

    reveal(&window, &mut cx, FILES);

    assert_eq!(
        docks(&window, &mut cx),
        [
            (vec![FILES], Some(FILES), true),
            (vec![BACKLINKS], Some(BACKLINKS), false),
        ]
    );
}

#[gpui::test]
fn a_moved_view_becomes_active_and_its_dock_shows(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    toggle(&window, &mut cx, Side::Right);

    move_view(&window, &mut cx, FILES, Side::Right, 0);

    assert_eq!(
        docks(&window, &mut cx),
        [
            (Vec::new(), None, true),
            (vec![FILES, BACKLINKS], Some(FILES), true),
        ]
    );
}

#[gpui::test]
fn the_right_neighbour_takes_the_active_slot(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, FILES);

    move_view(&window, &mut cx, FILES, Side::Right, 0);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![BACKLINKS], Some(BACKLINKS), true)
    );
}

#[gpui::test]
fn the_left_neighbour_takes_it_when_the_active_was_last(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, BACKLINKS);

    move_view(&window, &mut cx, BACKLINKS, Side::Right, 0);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![FILES], Some(FILES), true)
    );
}

#[gpui::test]
fn removing_a_view_before_the_active_keeps_the_active_view(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, BACKLINKS);

    move_view(&window, &mut cx, FILES, Side::Right, 0);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![BACKLINKS], Some(BACKLINKS), true)
    );
}

#[gpui::test]
fn a_move_inside_a_dock_reorders(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, FILES);

    move_view(&window, &mut cx, BACKLINKS, Side::Left, 0);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![BACKLINKS, FILES], Some(FILES), true)
    );
}

#[gpui::test]
#[should_panic(expected = "past the end")]
fn an_index_past_the_end_panics(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);

    move_view(&window, &mut cx, FILES, Side::Right, 2);
}

#[gpui::test]
#[should_panic(expected = "past the end")]
fn an_index_past_the_end_of_its_own_dock_panics(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, FILES);

    move_view(&window, &mut cx, FILES, Side::Left, 2);
}

#[gpui::test]
fn append_on_the_same_side_moves_to_the_end(cx: &mut TestAppContext) {
    let (window, mut cx) = both_left(cx, BACKLINKS);

    append(&window, &mut cx, FILES, Side::Left);

    assert_eq!(
        left(docks(&window, &mut cx)),
        (vec![BACKLINKS, FILES], Some(BACKLINKS), true)
    );
}

#[gpui::test]
fn append_on_the_other_side_moves_to_the_end(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);

    append(&window, &mut cx, FILES, Side::Right);

    assert_eq!(
        docks(&window, &mut cx),
        [
            (Vec::new(), None, true),
            (vec![BACKLINKS, FILES], Some(FILES), true),
        ]
    );
}
