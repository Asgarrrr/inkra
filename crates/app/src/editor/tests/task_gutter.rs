//! Task gutters (task boxes spec, D2 and D3). Headless, a body glyph is 10.8 px, so `- [ ] ` is 64.8,
//! wider than its one slot.

use gpui::{Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, point, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::theme::{LIST_SLOT_GAP, TASK_BOX_SIZE};

const GLYPH: f32 = 0.6 * 18.;

/// What each piece of line 0's prefix paints, from its left to its right.
type Pieces = Vec<(String, Pixels, Pixels)>;

fn slot() -> f32 {
    f32::from(TASK_BOX_SIZE + LIST_SLOT_GAP)
}

fn pieces(layout: &crate::editor::layout::DocumentLayout) -> Pieces {
    let pieces = layout.prefix_shown(0).into_iter();
    pieces
        .map(|(text, left, right)| (text.to_owned(), left, right))
        .collect()
}

/// Every row top and the `x` of each offset in `offsets`, and what line 0's prefix paints.
fn geometry(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    offsets: &[usize],
) -> (Pieces, Vec<Pixels>, Vec<Pixels>) {
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
            let shown = pieces(layout);
            (shown, tops, xs)
        })
        .unwrap()
}

/// `cmd-up` puts the caret on the `-`: the gutter widens to the raw item part, so row 0's text and
/// every wrapped row start the raw width less one slot further right.
#[gpui::test]
fn revealing_a_task_part_widens_its_gutter(cx: &mut TestAppContext) {
    let text = format!("- [ ] {}\nbody", "aaaa ".repeat(40));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let (shown, concealed) = row_starts(&window, &mut cx);
    assert_eq!(shown, []);
    assert!(concealed.len() > 2, "the line wraps: {concealed:?}");
    for &x in &concealed {
        assert_x(x, slot());
    }
    cx.simulate_keystrokes("cmd-up");
    let (shown, revealed) = row_starts(&window, &mut cx);
    let [(text, left, raw)] = shown.as_slice() else {
        panic!("{shown:?}");
    };
    assert_eq!((text.as_str(), *left), ("- [ ] ", px(0.)));
    assert!(revealed.len() > 2, "the line wraps: {revealed:?}");
    for &x in &revealed {
        assert_x(x - concealed[0], f32::from(*raw) - slot());
    }
}

/// The `x` of each row start of line 0, the text's start on row 0, and what its prefix paints.
fn row_starts(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
) -> (Pieces, Vec<Pixels>) {
    window
        .update(cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let last = layout.row_for_offset(at(6 + 200));
            let wrapped = (1..=last).map(|row| layout.offset_on_row(row, px(0.)));
            let xs = std::iter::once(at(6))
                .chain(wrapped)
                .map(|offset| layout.x_for_offset(offset))
                .collect();
            let shown = pieces(layout);
            (shown, xs)
        })
        .unwrap()
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
    assert_eq!(shown, []);
    let line = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().shown(0).to_owned()
        })
        .unwrap();
    assert_eq!(line, "a", "the marker stays in the gutter, out of the text");
    assert_x(unchecked[0], slot());
    assert_eq!(checked, unchecked);
}

/// `> - [ ] a` with the lazy line `  b`: a caret on the `>` paints the quote part alone and moves no
/// text, one at the bullet the item part alone at its own `x`, its text at the raw part's end, and
/// `b`'s line starts under `a`, its blanks hidden. A selection over both paints each part apart.
#[gpui::test]
fn the_quote_part_and_the_task_part_reveal_apart(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\n  b\n\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    let (shown, _, xs) = geometry(&window, &mut cx, &[8, 12]);
    assert_eq!(shown, []);
    assert_x(xs[0], 2. * GLYPH + slot());
    assert_eq!(xs[1], xs[0]);
    cx.simulate_keystrokes("cmd-up");
    let (shown, _, quote_xs) = geometry(&window, &mut cx, &[8, 12]);
    let [(text, x, quote_end)] = shown.as_slice() else {
        panic!("{shown:?}");
    };
    assert_eq!((text.as_str(), *x), ("> ", px(0.)));
    assert_x(*quote_end, 2. * GLYPH);
    assert_eq!(quote_xs, xs);
    cx.simulate_keystrokes("right right");
    let (shown, _, task_xs) = geometry(&window, &mut cx, &[8, 12]);
    let [(text, x, end)] = shown.as_slice() else {
        panic!("{shown:?}");
    };
    assert_eq!(text, "- [ ] ");
    assert_x(*x, 2. * GLYPH);
    assert_eq!(*end, task_xs[0]);
    assert_eq!(task_xs[1], xs[1]);
    cx.simulate_keystrokes("shift-left");
    let (shown, _, both_xs) = geometry(&window, &mut cx, &[8]);
    let texts: Vec<_> = shown.iter().map(|piece| piece.0.as_str()).collect();
    assert_eq!(texts, ["> ", "- [ ] "]);
    assert_eq!(shown[0].2, *quote_end);
    assert_x(shown[1].1, 2. * GLYPH);
    assert_eq!(shown[1].2, both_xs[0]);
    assert_eq!(both_xs[0], task_xs[0]);
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
    assert_x(task_start, 2. * GLYPH + slot());
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
