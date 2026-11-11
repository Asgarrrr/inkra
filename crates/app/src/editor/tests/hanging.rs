//! D12: a revealed heading's opening marker hangs left of the text. Headless, an H1 glyph is 20.4 px, so `# ` is 40.8.

use gpui::{Modifiers, TestAppContext, point, px};

use super::geometry::H1_LINE_HEIGHT as H1;
use super::{assert_x, at, dispatching};
use crate::text_input::InputHost;

const H1_GLYPH: f32 = 0.6 * 34.;

/// 36 chars fit an H1 row (734.4 px of 756); with `# ` inline, 38 wrapped onto a second.
#[gpui::test]
fn revealing_a_heading_keeps_every_row(cx: &mut TestAppContext) {
    let text = format!("# {}a\nbody", "aaaa ".repeat(7));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    let rows = |cx: &mut gpui::VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                (0..=layout.row_count())
                    .map(|row| layout.top_for_row(row))
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    let concealed = rows(&mut cx);
    cx.simulate_keystrokes("cmd-up");
    let revealed = rows(&mut cx);
    assert_eq!(concealed, [0., H1, H1 + 28.].map(px));
    assert_eq!(revealed, concealed);
}

#[gpui::test]
fn a_caret_in_the_marker_hangs_left_of_the_text(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let inside = layout.caret_bounds(at(1));
            assert_x(inside.origin.x, -H1_GLYPH);
            assert_eq!(inside.size.height, px(H1));
            assert_x(layout.caret_bounds(at(0)).origin.x, -2. * H1_GLYPH);
            assert_eq!(layout.caret_bounds(at(2)).origin.x, px(0.));
        })
        .unwrap();
}

/// `x + 40.8` is 15.8 px into the marker, nearer the second glyph's start (20.4) than the first's.
#[gpui::test]
fn a_click_left_of_the_text_lands_in_a_revealed_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();

    cx.simulate_click(text.origin + point(px(-25.), px(10.)), Modifiers::none());
    let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(caret, at(1));

    cx.simulate_click(
        text.origin + point(px(-25.), px(H1 + 10.)),
        Modifiers::none(),
    );
    let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(caret, at(4));
}

/// At x = 0 and just under half a glyph (10.2 px), `Bias::Left` alone would land at `line.start`, left of the `#` (D12).
#[gpui::test]
fn a_click_on_a_heading_text_start_lands_after_the_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    for x in [0., 10., 0., 10.] {
        // The first two clicks hit a concealed marker; each lands on the heading, so the rest hit a revealed one.
        cx.simulate_click(text.origin + point(px(x), px(10.)), Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(caret, at(2), "x = {x}");
    }
}

#[gpui::test]
fn up_from_a_body_line_start_lands_after_the_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("home up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(2)))
        .unwrap();
}

#[gpui::test]
fn a_selection_across_a_heading_covers_its_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "x\n# T\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let quads = layout.selection_quads(&(at(0)..at(10)), &(px(0.)..layout.height()));
            let lefts: Vec<_> = quads.iter().map(|quad| quad.bounds.origin.x).collect();
            assert_eq!(lefts[0], px(0.));
            assert_x(lefts[1], -2. * H1_GLYPH);
            assert_eq!(lefts[2], px(0.));
        })
        .unwrap();
}

#[gpui::test]
fn a_selection_from_a_heading_start_begins_in_the_margin(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let quads = layout.selection_quads(&(at(0)..at(8)), &(px(0.)..layout.height()));
            assert_x(quads[0].bounds.origin.x, -2. * H1_GLYPH);
        })
        .unwrap();
}

#[gpui::test]
fn the_ime_rectangle_of_a_marker_range_is_in_the_margin(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, window, cx| {
            let origin = view.input.last_bounds().unwrap();
            let bounds = view.bounds_for_range(0..1, origin, window, cx).unwrap();
            assert_x(bounds.origin.x - origin.origin.x, -2. * H1_GLYPH);
            assert_x(bounds.size.width, H1_GLYPH);
            assert_eq!(bounds.size.height, px(H1));
        })
        .unwrap();
}

#[gpui::test]
fn a_heading_inside_a_quote_keeps_its_inline_conceal_after_the_gutter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> # q\nx");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(view.cursor(), at(2));
            assert_eq!(layout.shown(0), "# q");
            assert_x(layout.x_for_offset(at(2)), 2. * 0.6 * 18.);
        })
        .unwrap();
}

/// `#` alone shows no text: a click right of the margin lands after the marker, one left of it before.
#[gpui::test]
fn a_click_on_an_empty_heading_lands_beside_its_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "#\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.offset_for_position(point(px(5.), px(10.))), at(1));
            assert_eq!(layout.offset_for_position(point(px(-15.), px(10.))), at(0));
        })
        .unwrap();
}

#[gpui::test]
fn home_on_a_revealed_heading_goes_before_its_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right right home");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.cursor(), at(0));
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.caret_bounds(at(0)).origin.x, -2. * H1_GLYPH);
        })
        .unwrap();
}

/// Down conceals the heading again; up must still reach the margin the goal x came from.
#[gpui::test]
fn a_goal_in_the_margin_returns_to_a_concealed_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up down up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();
}

#[gpui::test]
fn a_click_in_the_margin_of_a_concealed_heading_lands_before_its_marker(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.offset_for_position(point(px(-5.), px(1.))), at(0));
        })
        .unwrap();
}
