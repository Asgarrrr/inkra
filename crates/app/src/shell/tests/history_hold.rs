//! The hold: a 500 ms left press on an arrow opens the history menu; the click its release makes walks nothing.

use gpui::{
    GestureTuning, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point,
    px,
};

use super::dock_menu::drawn;
use super::tab_bar::click;
use super::{ab_focused, four_notes, keys, open, showing};

fn center(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.debug_bounds(selector).expect("drawn").center()
}

fn elapse(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(GestureTuning::default().long_press_duration);
    cx.run_until_parked();
}

fn hold_at(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    elapse(cx);
}

#[gpui::test]
fn a_hold_on_an_arrow_opens_the_history_menu_and_its_release_walks_nothing(
    cx: &mut TestAppContext,
) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");

    hold_at(&mut cx, at);
    assert!(drawn(&mut cx, "context-menu"));

    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(drawn(&mut cx, "context-menu"));
    assert_eq!(showing(&window, &mut cx), Some(c));
}

#[gpui::test]
fn a_hold_closed_by_escape_then_released_on_the_arrow_walks_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");

    hold_at(&mut cx, at);
    keys(&mut cx, "escape");
    assert!(!drawn(&mut cx, "context-menu"));
    // A key leaves the window in keyboard modality, where nothing is hovered and no click fires.
    cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(showing(&window, &mut cx), Some(c));

    click(&mut cx, "nav-back");
    assert_eq!(showing(&window, &mut cx), Some(b));
}

#[gpui::test]
fn a_short_click_walks_and_opens_no_menu_later(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }

    click(&mut cx, "nav-back");
    assert_eq!(showing(&window, &mut cx), Some(b));
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_short_press_on_a_dimmed_arrow_opens_no_menu_later(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    let at = center(&mut cx, "nav-forward-disabled");

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_hold_on_a_dimmed_arrow_opens_the_history_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    let at = center(&mut cx, "nav-forward-disabled");

    hold_at(&mut cx, at);
    assert!(drawn(&mut cx, "context-menu"));

    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(drawn(&mut cx, "context-menu"));
    assert_eq!(showing(&window, &mut cx), Some(b));
}

#[gpui::test]
fn a_press_dragged_beyond_the_slop_opens_no_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");
    let away = at + point(px(0.), GestureTuning::default().touch_slop + px(4.));

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(away, MouseButton::Left, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
    cx.simulate_mouse_up(away, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn a_press_released_off_the_arrow_opens_no_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");
    let editor = center(&mut cx, "note-column");

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(editor, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(editor, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at, None, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_press_released_just_off_the_arrow_opens_no_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let arrow = cx.debug_bounds("nav-back").expect("drawn");
    let at = point(arrow.left() + px(1.), arrow.center().y);
    let off = point(arrow.left() - px(3.), arrow.center().y);

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(off, MouseButton::Left, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_press_moved_out_and_back_opens_no_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");
    let away = at + point(px(0.), GestureTuning::default().touch_slop + px(4.));

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(away, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_press_whose_button_is_gone_opens_no_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let at = center(&mut cx, "nav-back");

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    // A release outside the window reaches no listener; the next move reports no button.
    cx.simulate_mouse_move(at, None, Modifiers::none());
    elapse(&mut cx);

    assert!(!drawn(&mut cx, "context-menu"));
}

#[gpui::test]
fn a_hold_whose_group_closes_opens_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    super::splits::split(&mut cx);
    open(&window, &mut cx, a);
    let at = center(&mut cx, "nav-back-1");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    keys(&mut cx, "cmd-w");
    assert_eq!(super::splits::centre(&window, &mut cx).len(), 1);
    elapse(&mut cx);
    assert!(!drawn(&mut cx, "context-menu"));
    super::splits::close(&mut cx);
}
