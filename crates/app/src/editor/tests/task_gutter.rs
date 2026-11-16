//! Task gutters (task boxes spec, D2 and D3). Headless, a body glyph is 10.8 px, so `- [ ] ` is 64.8.

use gpui::{Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, point, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;

const GLYPH: f32 = 0.6 * 18.;

/// Every row top and the `x` of each offset in `offsets`, and what line 0's prefix paints.
fn geometry(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    offsets: &[usize],
) -> (Option<(String, Pixels)>, Vec<Pixels>, Vec<Pixels>) {
    window
        .update(cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let tops = (0..=layout.row_count())
                .map(|row| layout.top_for_row(row))
                .collect();
            let xs = offsets
                .iter()
                .map(|&offset| layout.x_for_offset(at(offset)))
                .collect();
            let shown = layout.prefix_shown(0).map(|(text, x)| (text.to_owned(), x));
            (shown, tops, xs)
        })
        .unwrap()
}

/// `cmd-up` puts the caret on the `-`: the task part paints in the gutter, and the text keeps every
/// row and every x, its wrapped row's included.
#[gpui::test]
fn revealing_a_task_part_moves_no_text(cx: &mut TestAppContext) {
    let text = format!("- [ ] {}\nbody", "aaaa ".repeat(20));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let (shown, concealed, xs) = geometry(&window, &mut cx, &[6, 66]);
    assert_eq!(shown, None);
    assert!(concealed.len() > 3, "the line wraps: {concealed:?}");
    assert_x(xs[0], 6. * GLYPH);
    assert_x(xs[1], 6. * GLYPH);
    cx.simulate_keystrokes("cmd-up");
    let (shown, revealed, revealed_xs) = geometry(&window, &mut cx, &[6, 66]);
    assert_eq!(shown, Some(("- [ ] ".to_owned(), px(0.))));
    assert_eq!((revealed, revealed_xs), (concealed, xs));
}

/// `[ ]` becomes `[x]` in the buffer: the line keeps its task gutter and its width.
#[gpui::test]
fn toggling_a_task_keeps_its_gutter_width(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- [ ] a\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let (_, _, unchecked) = geometry(&window, &mut cx, &[6]);
    cx.simulate_keystrokes("cmd-up right right right shift-right");
    cx.simulate_input("x");
    cx.simulate_keystrokes("cmd-down");
    let (shown, _, checked) = geometry(&window, &mut cx, &[6]);
    let text = window
        .update(&mut cx, |view, _, cx| {
            view.buffer().read(cx).text().to_owned()
        })
        .unwrap();
    assert_eq!(text, "- [x] a\nbody");
    assert_eq!(shown, None);
    let line = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().shown(0).to_owned()
        })
        .unwrap();
    assert_eq!(line, "a", "the marker stays in the gutter, out of the text");
    assert_x(unchecked[0], 6. * GLYPH);
    assert_eq!(checked, unchecked);
}

/// `> - [ ] a` with the lazy line `  b`: a caret on the `>` paints the quote part alone, one at the
/// bullet the task part alone at its own `x`, and `b`'s line starts at the quote part's edge.
#[gpui::test]
fn the_quote_part_and_the_task_part_reveal_apart(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\n  b\n\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let (shown, _, xs) = geometry(&window, &mut cx, &[8, 10]);
    assert_eq!(shown, None);
    assert_x(xs[0], 8. * GLYPH);
    assert_x(xs[1], 2. * GLYPH);
    cx.simulate_keystrokes("cmd-up");
    let (shown, _, quote_xs) = geometry(&window, &mut cx, &[8, 10]);
    assert_eq!(shown, Some(("> ".to_owned(), px(0.))));
    assert_eq!(quote_xs, xs);
    cx.simulate_keystrokes("right right");
    let (shown, _, task_xs) = geometry(&window, &mut cx, &[8, 10]);
    let (text, x) = shown.unwrap();
    assert_eq!(text, "- [ ] ");
    assert_x(x, 2. * GLYPH);
    assert_eq!(task_xs, xs);
    cx.simulate_keystrokes("shift-left");
    let (shown, _, _) = geometry(&window, &mut cx, &[]);
    assert_eq!(shown, Some(("> - [ ] ".to_owned(), px(0.))));
}

/// With the task part revealed, a click on its `[` lands on it, and one in the concealed quote part
/// on the text's start.
#[gpui::test]
fn a_click_lands_through_the_revealed_part_only(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right");
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    for (x, expected) in [(4.2 * GLYPH, 4), (0.5 * GLYPH, 8)] {
        cx.simulate_keystrokes("cmd-up right right");
        cx.simulate_click(text.origin + point(px(x), px(10.)), Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(caret, at(expected), "x = {x}");
    }
}

/// A shown part maps its own offsets, at their `x` in the whole gutter; an offset in a concealed part
/// maps to the text's start.
#[gpui::test]
fn a_shown_part_maps_its_offsets_where_they_stand(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\nbody");
    cx.run_until_parked();
    let xs = |cx: &mut VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                (
                    layout.caret_bounds(view.cursor()).origin.x,
                    layout.x_for_offset(at(2)),
                )
            })
            .unwrap()
    };
    cx.simulate_keystrokes("cmd-up");
    let (caret, task_start) = xs(&mut cx);
    assert_x(caret, 0.);
    assert_x(task_start, 8. * GLYPH);
    cx.simulate_keystrokes("right right right");
    let (caret, task_start) = xs(&mut cx);
    assert_x(caret, 3. * GLYPH);
    assert_x(task_start, 2. * GLYPH);
}

/// With the quote part shown: left of the column lands on the `>`, past its space's start on the
/// text, not on the concealed task part.
#[gpui::test]
fn a_click_on_a_shown_quote_part_stays_out_of_the_task_part(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\nbody");
    cx.run_until_parked();
    let text = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    for (x, expected) in [(-5., 0), (1.7 * GLYPH, 8)] {
        cx.simulate_keystrokes("cmd-up");
        cx.simulate_click(text.origin + point(px(x), px(10.)), Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(caret, at(expected), "x = {x}");
    }
}
