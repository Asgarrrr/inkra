//! Quote gutters (markdown blocks spec, D3). Headless, a body glyph is 10.8 px, so `> ` is 21.6.

use gpui::{Bounds, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px};

use super::{assert_x, at, dispatching};
use crate::editor::layout::shape_count;
use crate::text_input::InputHost;

const GLYPH: f32 = 0.6 * 18.;

/// 100 chars in five-char words beside `> `: 68 fit a row (734.4 px), so the wrap falls at 65.
fn wrapped_quote() -> String {
    format!("> {}\nbody", "aaaa ".repeat(20))
}

#[gpui::test]
fn a_wrapped_quote_line_starts_every_row_at_the_gutter_edge(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &wrapped_quote());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert!(layout.shown(0).starts_with("aaaa"));
            assert_eq!(layout.row_count(), 3);
            assert_eq!(*layout.row_bounds(1).start(), at(67));
            assert_x(layout.x_for_offset(at(2)), 2. * GLYPH);
            assert_x(layout.x_for_offset(at(67)), 2. * GLYPH);
        })
        .unwrap();
}

#[gpui::test]
fn nested_quotes_indent_by_both_markers(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> > x\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(0), "x");
            assert_x(layout.x_for_offset(at(4)), 4. * GLYPH);
        })
        .unwrap();
}

/// Rows 0 and 1 of the wrapped line: `x < 21.6` is the gutter's, and it lands on the row's text start.
#[gpui::test]
fn a_click_in_the_gutter_lands_at_the_text_start(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &wrapped_quote());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    for (x, y, expected) in [(5., 10., 2), (-5., 10., 2), (5., 38., 67)] {
        cx.simulate_click(text.origin + point(px(x), px(y)), Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(caret, at(expected), "({x}, {y})");
    }
}

#[gpui::test]
fn down_from_a_body_line_start_lands_at_the_quote_text(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "x\n> quote");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(4)))
        .unwrap();
}

/// A selection across a quote line starts at the gutter's left; a wrapped row and the lazy `body` at its edge.
#[gpui::test]
fn a_selection_across_a_quote_covers_its_gutter(cx: &mut TestAppContext) {
    let text = wrapped_quote();
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let quads = layout.selection_quads(&(at(0)..at(text.len())));
            let lefts: Vec<_> = quads.iter().map(|quad| quad.bounds.origin.x).collect();
            assert_eq!(lefts[0], px(0.));
            assert_x(lefts[1], 2. * GLYPH);
            assert_x(lefts[2], 2. * GLYPH);
        })
        .unwrap();
}

#[gpui::test]
fn a_lazy_line_starts_at_the_gutter_above(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> > a\nb");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.x_for_offset(at(6)), 4. * GLYPH);
        })
        .unwrap();
}

/// `b` has one `>` of its two levels: its text still starts under `a`.
#[gpui::test]
fn a_line_short_of_its_depth_starts_at_the_gutter_above(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> > a\n> b");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(1), "b");
            assert_x(layout.x_for_offset(at(8)), 4. * GLYPH);
        })
        .unwrap();
}

/// `cmd-up` puts the caret on the `>`: it paints, and the text keeps every row and every x (G5).
#[gpui::test]
fn revealing_a_quote_marker_moves_no_text(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &wrapped_quote());
    cx.run_until_parked();
    let geometry = |cx: &mut VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                let tops: Vec<_> = (0..=layout.row_count())
                    .map(|row| layout.top_for_row(row))
                    .collect();
                let xs = (layout.x_for_offset(at(2)), layout.x_for_offset(at(67)));
                (layout.prefix_painted(0), tops, xs)
            })
            .unwrap()
    };
    cx.simulate_keystrokes("cmd-down");
    let (painted, concealed, xs) = geometry(&mut cx);
    assert!(!painted);
    cx.simulate_keystrokes("cmd-up");
    let (painted, revealed, revealed_xs) = geometry(&mut cx);
    assert!(painted);
    assert_eq!((revealed, revealed_xs), (concealed, xs));
}

/// `x = 8` is nearer the `>`'s end (10.8) than its start; `x = 3` nearer its start. Past the space's
/// start (10.8) gpui's `closest_index_for_x` returns the length, as in D12's margin.
#[gpui::test]
fn a_click_on_a_revealed_quote_marker_lands_on_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> quote\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    for (x, expected) in [(8., 1), (3., 0)] {
        cx.simulate_click(text.origin + point(px(x), px(10.)), Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(caret, at(expected), "x = {x}");
    }
}

/// A concealed gutter keeps no shape: an offset inside it maps to the text's start, as a concealed
/// margin's does.
#[gpui::test]
fn a_caret_after_the_gutter_leaves_it_concealed(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> > quote\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right right right");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(view.cursor(), at(4));
            assert!(!layout.prefix_painted(0));
            assert_x(layout.caret_bounds(at(3)).origin.x, 4. * GLYPH);
        })
        .unwrap();
}

/// `2.` is `QUOTE_BAR_WIDTH`, written out as `tests/mod.rs`'s `geometry` module does.
fn bar(x: f32, top: f32, bottom: f32) -> Bounds<Pixels> {
    Bounds::from_corners(point(px(x), px(top)), point(px(x + 2.), px(bottom)))
}

/// The wrapped line's bar covers both its rows and meets the next line's; the blank line ends the quote.
#[gpui::test]
fn a_quote_bar_runs_from_its_line_top_to_the_next_line(cx: &mut TestAppContext) {
    let text = format!("> {}\n> b\n\nbody", "aaaa ".repeat(20));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let bars = layout.quote_bars();
            assert_eq!(bars, [bar(0., 0., 56.), bar(0., 56., 84.)]);
        })
        .unwrap();
}

#[gpui::test]
fn nested_quotes_draw_a_bar_per_level_and_a_lazy_line_keeps_them(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> > a\nb\n\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let lefts: Vec<_> = layout
                .quote_bars()
                .iter()
                .map(|bar| f32::from(bar.origin.x))
                .collect();
            assert_eq!(lefts.len(), 4);
            for (left, expected) in lefts.iter().zip([0., 2. * GLYPH, 0., 2. * GLYPH]) {
                assert!((left - expected).abs() < 0.01, "{lefts:?}");
            }
        })
        .unwrap();
}

/// The `>` conceals join the touched set, so stepping onto them reshapes once and stepping off once.
#[gpui::test]
fn a_caret_onto_and_off_a_quote_marker_shapes_once_each(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "> quote\nbody");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);

    let before = shape_count();
    cx.simulate_keystrokes("right right");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

/// A marked range after the gutter: its rectangle starts at the text, right of the gutter.
#[gpui::test]
fn the_ime_rectangle_on_a_quoted_line_is_right_of_the_gutter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> quote\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, window, cx| {
            let origin = view.input.last_bounds().unwrap();
            let bounds = view.bounds_for_range(2..7, origin, window, cx).unwrap();
            assert_x(bounds.origin.x - origin.origin.x, 2. * GLYPH);
            assert_x(bounds.size.width, 5. * GLYPH);
        })
        .unwrap();
}

/// Both lines hold one `>` of one quote, but their gutter texts differ: each measures its own width.
#[gpui::test]
fn gutters_of_different_text_measure_their_own_width(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> a\n  > b\n> c");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.x_for_offset(at(2)), 2. * GLYPH);
            assert_x(layout.x_for_offset(at(8)), 4. * GLYPH);
            assert_x(layout.x_for_offset(at(12)), 2. * GLYPH);
        })
        .unwrap();
}
