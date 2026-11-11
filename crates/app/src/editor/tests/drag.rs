//! A drag selection follows the pointer after it leaves the editor; a hover after a lost
//! release, or a drag begun outside, selects nothing.

use gpui::{
    Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, WindowHandle,
};
use gpui::{point, px};

use super::{at, close, dispatching};
use crate::editor::EditorView;

const NOTE: &str = "first\nsecond\nthird";

/// A window point inside row 1, at `offset`'s x.
fn on_row_1(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Point<Pixels> {
    window
        .update(cx, |view, _, _| {
            let origin = view.text_bounds().unwrap().origin;
            let layout = view.input.last_layout().unwrap();
            origin
                + point(
                    layout.x_for_offset(at(offset)),
                    layout.top_for_row(1) + px(14.),
                )
        })
        .unwrap()
}

#[gpui::test]
fn a_drag_above_the_editor_selects_to_the_start(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, NOTE);
    cx.run_until_parked();
    let from = on_row_1(window, &mut cx, 9);
    let outside = point(px(-20.), px(-20.));

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    let chosen = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let selected = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    assert_eq!(selected.start, at(0));
    assert_eq!(selected.end, chosen.end);
    assert!(chosen.end > at(6), "the mouse down missed row 1");
    close(&mut cx);
}

#[gpui::test]
fn a_hover_after_a_lost_release_selects_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, NOTE);
    cx.run_until_parked();
    let from = on_row_1(window, &mut cx, 9);
    let elsewhere = on_row_1(window, &mut cx, 7);

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    let before = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    cx.simulate_mouse_move(elsewhere, None, Modifiers::none());
    cx.run_until_parked();

    let after = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    assert_eq!(
        after, before,
        "a hover after a lost release moved the selection"
    );
    cx.simulate_mouse_up(elsewhere, MouseButton::Left, Modifiers::none());
    close(&mut cx);
}

#[gpui::test]
fn a_drag_begun_outside_the_editor_selects_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, NOTE);
    cx.run_until_parked();
    let elsewhere = on_row_1(window, &mut cx, 7);

    cx.simulate_mouse_move(elsewhere, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let selected = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    assert!(
        selected.is_empty(),
        "a drag begun outside selected {selected:?}"
    );
    close(&mut cx);
}
