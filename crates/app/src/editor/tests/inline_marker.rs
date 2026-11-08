//! D13: a revealed marker wider than the margin shows inline. Headless, `# ` is 40.8 px and `#### ` 60.

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle, point, px, size};

use super::geometry::{H1_LINE_HEIGHT as H1, PADDING};
use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::element::column_width;
use crate::editor::layout::shape_count;

/// `text` in a pane wide enough for the column and `margin` on each side.
fn with_margin(
    cx: &mut TestAppContext,
    text: &str,
    margin: f32,
) -> (WindowHandle<EditorView>, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.run_until_parked();
    resize_to(window, &mut cx, margin);
    (window, cx)
}

fn resize_to(window: WindowHandle<EditorView>, cx: &mut VisualTestContext, margin: f32) {
    let column = window
        .update(cx, |view, window, _| {
            let style = view.input.last_layout().unwrap().style().clone();
            f32::from(column_width(&style, window))
        })
        .unwrap();
    cx.simulate_resize(size(px(2. * PADDING + column + 2. * margin), px(600.)));
    cx.run_until_parked();
}

fn caret(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> inkra_text::ByteOffset {
    window.update(cx, |view, _, _| view.cursor()).unwrap()
}

#[gpui::test]
fn a_marker_hangs_only_where_it_fits(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "# T\n#### T\nbody", 50.);
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert!(layout.prefix_painted(0));
            assert_eq!(layout.x_for_offset(at(2)), px(0.));
        })
        .unwrap();

    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(0), "T");
            assert!(!layout.prefix_painted(1));
            assert_eq!(layout.shown(1), "#### T");
            assert_x(layout.caret_bounds(at(9)).origin.x, 60.);
        })
        .unwrap();

    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    cx.simulate_click(
        text.origin + point(px(59.), px(H1 + 10.)),
        Modifiers::none(),
    );
    assert_eq!(caret(window, &mut cx), at(9));
}

/// Back on the line the marker is concealed, so `x = 0` lands after it.
#[gpui::test]
fn home_down_up_on_an_inline_heading_lands_after_the_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "#### T\nbody", 50.);
    cx.simulate_keystrokes("cmd-up end home");
    assert_eq!(caret(window, &mut cx), at(0));
    cx.simulate_keystrokes("down");
    assert_eq!(caret(window, &mut cx), at(7));
    cx.simulate_keystrokes("up");
    assert_eq!(caret(window, &mut cx), at(5));
}

#[gpui::test]
fn home_down_up_on_a_hanging_heading_comes_back_to_its_start(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "#### T\nbody", 70.);
    cx.simulate_keystrokes("cmd-up end home down up");
    assert_eq!(caret(window, &mut cx), at(0));
}

#[gpui::test]
fn a_resize_in_the_band_reshapes_only_a_revealed_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "# T\nbody", 50.);
    let before = shape_count();
    resize_to(window, &mut cx, 40.);
    assert_eq!(shape_count() - before, 0, "nothing revealed");

    cx.simulate_keystrokes("cmd-up");
    let before = shape_count();
    resize_to(window, &mut cx, 30.);
    assert!(shape_count() > before, "`# ` revealed");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert!(!layout.prefix_painted(0));
            assert_eq!(layout.shown(0), "# T");
        })
        .unwrap();
}

#[gpui::test]
fn a_keystroke_on_a_heading_in_the_band_shapes_once(cx: &mut TestAppContext) {
    for margin in [50., 30.] {
        let (_window, mut cx) = with_margin(cx, "# T\nbody", margin);
        cx.simulate_keystrokes("cmd-up end");
        cx.run_until_parked();
        let before = shape_count();
        cx.simulate_keystrokes("x");
        cx.run_until_parked();
        assert_eq!(shape_count() - before, 1, "margin {margin}");
    }
}

#[gpui::test]
fn a_resize_in_the_band_ignores_a_heading_in_a_quote(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "> # q\nbody", 50.);
    cx.simulate_keystrokes("cmd-up end");
    let before = shape_count();
    resize_to(window, &mut cx, 30.);
    assert_eq!(shape_count() - before, 0, "`> # q` never hangs");
}

/// The resize reused a layout with no revealed marker; the reveal must shape at the new margin.
#[gpui::test]
fn a_reveal_after_a_resize_in_the_band_shapes_once(cx: &mut TestAppContext) {
    let (window, mut cx) = with_margin(cx, "#### T\nbody", 70.);
    cx.simulate_keystrokes("cmd-up down");
    resize_to(window, &mut cx, 50.);
    let before = shape_count();
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.input.last_layout().unwrap().shown(0), "#### T");
        })
        .unwrap();
}
