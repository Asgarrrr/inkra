//! A caret move or an edit inside a code block scrolls it to keep the caret visible (code block
//! scroll spec, S2). Headless, a code glyph is 9.6 px against a 756 px column; the margin is 16.

use gpui::{Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext, WindowHandle};
use gpui::{point, prelude::*, px};
use inkra_text::Buffer;

use super::code_block_scroll::{block, swipe_block};
use super::{assert_x, at, close, dispatching};
use crate::editor::EditorView;
use crate::text_input::painted_caret;

/// The painted caret's x from the text's left edge; `None` when no caret was painted.
fn painted_x(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Option<Pixels> {
    let left = window
        .update(cx, |view, _, _| view.text_bounds().unwrap().left())
        .unwrap();
    painted_caret().map(|caret| caret.left() - left)
}

fn block_x(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Pixels {
    window.update(cx, |view, _, _| view.block_x(0)).unwrap()
}

/// Block 0's scroll, then where its caret was painted.
fn assert_reveal(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    (x, caret): (f32, f32),
) {
    assert_x(block_x(window, cx), x);
    let Some(painted) = painted_x(window, cx) else {
        panic!("no caret was painted");
    };
    assert_x(painted, caret);
}

#[gpui::test]
fn typing_at_the_end_keeps_the_caret_in_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.simulate_keystrokes("cmd-up down end");
    cx.run_until_parked();
    assert_reveal(window, &mut cx, (412., 740.));

    cx.simulate_keystrokes("y");
    cx.run_until_parked();
    assert_reveal(window, &mut cx, (421.6, 740.));
    close(&mut cx);
}

#[gpui::test]
fn home_and_end_scroll_the_block(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.simulate_keystrokes("cmd-up down end");
    cx.run_until_parked();

    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    assert_reveal(window, &mut cx, (0., 0.));
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    assert_reveal(window, &mut cx, (412., 740.));
    close(&mut cx);
}

#[gpui::test]
fn down_onto_a_short_line_scrolls_back(cx: &mut TestAppContext) {
    let text = format!("```\n{}\n{}\n```\nbody", "x".repeat(120), "x".repeat(10));
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up down end down");
    cx.run_until_parked();
    assert_eq!(
        window.update(&mut cx, |view, _, _| view.cursor()).unwrap(),
        at(135)
    );
    assert_reveal(window, &mut cx, (96., 0.));
    close(&mut cx);
}

/// An indented block: a fence line would itself scroll the block back to its short end.
#[gpui::test]
fn down_from_prose_keeps_a_visible_caret_still(cx: &mut TestAppContext) {
    let text = format!(
        "a prose line long enough to set a goal\n\n    {}\n",
        "x".repeat(120)
    );
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up end");
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -96.);
    assert_x(block_x(window, &mut cx), 96.);
    let Some(goal) = painted_x(window, &mut cx) else {
        panic!("no caret on the prose line");
    };

    cx.simulate_keystrokes("down down");
    cx.run_until_parked();
    assert_x(block_x(window, &mut cx), 96.);
    let Some(caret) = painted_x(window, &mut cx) else {
        panic!("the caret was scrolled out");
    };
    assert!(
        (caret - goal).abs() <= px(4.8),
        "{caret:?} is not under {goal:?}"
    );
    close(&mut cx);
}

/// In a quote the text starts past the gutter: the reveal measures from the text's left.
#[gpui::test]
fn a_quoted_block_reveals_from_its_text_left(cx: &mut TestAppContext) {
    let text = format!("> ```\n> {}\n> ```\nbody", "x".repeat(120));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(98)..at(98), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    let background = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().block_backgrounds()[0]
        })
        .unwrap();
    assert!(background.left() > px(0.), "the block starts at the column");
    let visible = f32::from(background.size.width) - 16.;
    assert_reveal(
        window,
        &mut cx,
        (
            90. * 9.6 + 16. - visible,
            f32::from(background.right()) - 24.,
        ),
    );
    close(&mut cx);
}

#[gpui::test]
fn only_a_block_that_scrolls_has_a_span(cx: &mut TestAppContext) {
    let text = format!("```\nshort\n```\n```\n{}\n```", "x".repeat(120));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert!(layout.block_span(at(6)).is_none());
            assert!(layout.block_span(at(20)).is_some());
        })
        .unwrap();
}

#[gpui::test]
fn open_and_select_range_reveal_the_block(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "prose");
    cx.run_until_parked();

    let text = format!("```\n{}\n```\nbody", "x".repeat(100));
    window
        .update(&mut cx, |view, window, cx| {
            let buffer = cx.new(|_| Buffer::new(&text));
            view.open_buffer(buffer, None, cx);
            view.select_range(at(104)..at(104), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_x(block_x(window, &mut cx), 220.);
    assert!(painted_caret().is_some(), "the first frame hid the caret");
    close(&mut cx);
}

#[gpui::test]
fn a_drag_past_the_right_edge_reveals(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let (from, past) = window
        .update(&mut cx, |view, _, _| {
            let origin = view.text_bounds().unwrap().origin;
            let layout = view.input.last_layout().unwrap();
            let y = layout.top_for_row(1) + px(14.);
            let right = layout.block_backgrounds()[0].right();
            (
                origin + point(px(96.), y),
                origin + point(right + px(20.), y),
            )
        })
        .unwrap();

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(past, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(past, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let selected = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    assert_eq!(selected.start, at(14));
    assert!(selected.end > at(14), "the drag selected nothing");
    assert!(
        block_x(window, &mut cx) > px(0.),
        "the drag revealed nothing"
    );
    close(&mut cx);
}
