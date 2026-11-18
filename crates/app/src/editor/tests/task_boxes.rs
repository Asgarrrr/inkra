//! Task boxes (task boxes spec, D4's paint): one per line whose task part is concealed. Headless, a
//! body glyph is 10.8 px, so a box centres on the checkbox's middle glyph at a multiple of it.

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, WindowHandle, px, size};

use super::geometry::LINE_HEIGHT;
use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::painted_boxes;
use crate::theme::TASK_BOX_SIZE;

const GLYPH: f32 = 0.6 * 18.;

/// Each box the last paint drew, from the text's origin, and whether it is checked.
fn boxes(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
) -> Vec<(Bounds<Pixels>, bool)> {
    cx.run_until_parked();
    let origin = window
        .update(cx, |view, _, _| view.text_bounds().unwrap().origin)
        .unwrap();
    painted_boxes()
        .into_iter()
        .map(|task| {
            (
                Bounds::new(task.bounds.origin - origin, task.bounds.size),
                task.checked,
            )
        })
        .collect()
}

/// A box of its size centred on `x` and on the first row of the line whose top is `top`.
fn assert_box(bounds: Bounds<Pixels>, x: f32, top: f32) {
    assert_eq!(bounds.size, size(TASK_BOX_SIZE, TASK_BOX_SIZE));
    assert_x(bounds.center().x, x);
    assert_eq!(bounds.center().y, px(top + LINE_HEIGHT / 2.));
}

/// The box centres on the checkbox's middle glyph, the `X` it is measured with, on row 0 of a
/// wrapped line; every state paints at the same bounds, and only a checked one fills.
#[gpui::test]
fn a_concealed_checkbox_paints_one_box_per_state(cx: &mut TestAppContext) {
    let wrapped = "aaaa ".repeat(20);
    for (text, checked) in [("- [ ] ", false), ("- [x] ", true), ("- [X] ", true)] {
        let (window, mut cx) = dispatching(cx, &format!("{text}{wrapped}\nbody"));
        cx.simulate_keystrokes("cmd-down");
        let boxes = boxes(&window, &mut cx);
        let rows = window
            .update(&mut cx, |view, _, _| {
                view.input.last_layout().unwrap().row_count()
            })
            .unwrap();
        assert!(rows > 2, "the task line wraps: {rows} rows with `body`");
        assert_eq!(boxes.len(), 1, "{text}");
        let (bounds, painted) = boxes[0];
        assert_eq!(painted, checked, "{text}");
        assert_box(bounds, 3.5 * GLYPH, 0.);
    }
}

/// The box lies inside the gutter: right of the line's start and left of its text.
#[gpui::test]
fn a_box_lies_inside_its_gutter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "1. [ ] a\nbody");
    cx.simulate_keystrokes("cmd-down");
    let boxes = boxes(&window, &mut cx);
    let text = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().x_for_offset(at(7))
        })
        .unwrap();
    assert_eq!(boxes.len(), 1);
    let (bounds, _) = boxes[0];
    assert_box(bounds, 4.5 * GLYPH, 0.);
    assert!(bounds.left() >= px(0.), "{bounds:?}");
    assert!(
        bounds.right() <= text,
        "{bounds:?} past the text at {text:?}"
    );
}

/// A caret in the task part shows the checkbox as text, so no box paints over it.
#[gpui::test]
fn a_revealed_task_part_paints_no_box(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- [ ] a\nbody");
    cx.simulate_keystrokes("cmd-down cmd-up");
    assert_eq!(boxes(&window, &mut cx), []);
    cx.simulate_keystrokes("right right right right right");
    assert_eq!(boxes(&window, &mut cx), []);
    cx.simulate_keystrokes("right");
    assert_eq!(boxes(&window, &mut cx).len(), 1, "at the text start");
}

/// In `> - [ ] a`, a caret on the `>` reveals the quote part only: the box stays. The lazy line
/// under it carries the quote part's width and paints no box (spec D3, D).
#[gpui::test]
fn a_quoted_task_keeps_its_box_and_its_lazy_line_has_none(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] a\n  b\n\nbody");
    cx.simulate_keystrokes("cmd-down");
    let concealed = boxes(&window, &mut cx);
    assert_eq!(concealed.len(), 1);
    assert_box(concealed[0].0, 5.5 * GLYPH, 0.);
    cx.simulate_keystrokes("cmd-up");
    assert_eq!(boxes(&window, &mut cx), concealed);
    cx.simulate_keystrokes("right right");
    assert_eq!(boxes(&window, &mut cx), []);
}

/// Two tasks paint a box each, on their own line's first row.
#[gpui::test]
fn each_task_paints_its_box_on_its_own_line(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- [ ] a\n- [x] b\n\nbody");
    cx.simulate_keystrokes("cmd-down");
    let boxes = boxes(&window, &mut cx);
    assert_eq!(boxes.len(), 2);
    assert_box(boxes[0].0, 3.5 * GLYPH, 0.);
    assert_box(boxes[1].0, 3.5 * GLYPH, LINE_HEIGHT);
    assert_eq!((boxes[0].1, boxes[1].1), (false, true));
}
