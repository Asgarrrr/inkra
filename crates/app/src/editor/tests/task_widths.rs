//! Slots (task boxes spec, D3): a concealed item part takes one slot per mark, the box last; a
//! revealed one widens the gutter to its raw text where that is wider. Headless, a body glyph is
//! 10.8 px. Each concealed `x` is compared with another line's measured `x` or a slot's constants.

use gpui::{Pixels, TestAppContext, VisualTestContext, WindowHandle, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::{painted_boxes, painted_marks};
use crate::theme::{LIST_SLOT_GAP, TASK_BOX_SIZE};

const GLYPH: f32 = 0.6 * 18.;

/// What the last paint drew, from the text's origin: each box's left edge, and each mark's text
/// and the middle of its advance.
struct Painted {
    boxes: Vec<Pixels>,
    marks: Vec<(String, Pixels)>,
    xs: Vec<Pixels>,
}

/// The painted boxes and marks, and the `x` of each offset in `offsets`.
fn measure(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    offsets: &[usize],
) -> Painted {
    cx.run_until_parked();
    window
        .update(cx, |view, _, _| {
            let origin = view.text_bounds().unwrap().origin;
            let layout = view.input.last_layout().unwrap();
            let boxes = painted_boxes()
                .iter()
                .map(|task| task.bounds.left() - origin.x)
                .collect();
            let marks = painted_marks()
                .into_iter()
                .map(|mark| (mark.text, (mark.left + mark.right) / 2. - origin.x))
                .collect();
            let xs = offsets
                .iter()
                .map(|&offset| layout.x_for_offset(at(offset)))
                .collect();
            Painted { boxes, marks, xs }
        })
        .unwrap()
}

/// The editor on `text` with the caret on its last line, off every item part.
fn concealed(cx: &mut TestAppContext, text: &str) -> (WindowHandle<EditorView>, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    (window, cx)
}

fn slot() -> Pixels {
    TASK_BOX_SIZE + LIST_SLOT_GAP
}

/// `- [ ] a\n- b`: the box and `b`'s slot both start at 0, the `•` centred on the box's axis, and
/// `a` starts where `b` does, one slot right of the box.
#[gpui::test]
fn a_task_lines_up_with_a_sibling_bullet(cx: &mut TestAppContext) {
    let (window, mut cx) = concealed(cx, "- [ ] a\n- b\n\nbody");
    let Painted { boxes, marks, xs } = measure(&window, &mut cx, &[6, 10]);
    assert_eq!(boxes, [px(0.)]);
    assert_eq!(marks.len(), 1, "{marks:?}");
    assert_eq!(marks[0].0, "•");
    assert_x(marks[0].1, f32::from(TASK_BOX_SIZE / 2.));
    assert_x(xs[0], f32::from(boxes[0] + slot()));
    assert_eq!(xs[1], xs[0]);
}

/// Under `- [ ] a`, a nested task's box and a nested bullet's slot start where `a` does, and their
/// texts start at one `x`.
#[gpui::test]
fn a_child_of_a_task_sits_under_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = concealed(cx, "- [ ] a\n  - [ ] c\n  - d\n\nbody");
    let Painted { boxes, marks, xs } = measure(&window, &mut cx, &[6, 16, 22]);
    assert_eq!(boxes.len(), 2, "{boxes:?}");
    assert_eq!(boxes[1], xs[0]);
    assert_x(marks[0].1, f32::from(xs[0] + TASK_BOX_SIZE / 2.));
    assert_eq!(xs[1], xs[0] + slot());
    assert_eq!(xs[2], xs[1]);
}

/// `> - [ ] a\n> - b`: the box sits at the quote part's width, where `b`'s slot starts, and `a`
/// starts where `b` does.
#[gpui::test]
fn a_quoted_task_lines_up_with_a_quoted_bullet(cx: &mut TestAppContext) {
    let (window, mut cx) = concealed(cx, "> - [ ] a\n> - b\n\nbody");
    let Painted { boxes, marks, xs } = measure(&window, &mut cx, &[8, 14]);
    assert_eq!(boxes.len(), 1, "{boxes:?}");
    assert_x(boxes[0], 2. * GLYPH);
    assert_x(marks[0].1, f32::from(boxes[0] + TASK_BOX_SIZE / 2.));
    assert_x(xs[0], f32::from(boxes[0] + slot()));
    assert_eq!(xs[1], xs[0]);
}

/// A caret in the task part paints it raw at its shown `x`: the caret sits on its byte, and a
/// selection from it into the text runs to the text's byte past the raw part.
#[gpui::test]
fn a_caret_and_a_selection_in_a_revealed_task_part_sit_at_its_shown_x(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> - [ ] ab\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right right");
    let caret = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            layout.caret_bounds(view.cursor()).origin.x
        })
        .unwrap();
    assert_x(caret, 3. * GLYPH);
    cx.simulate_keystrokes(
        "shift-right shift-right shift-right shift-right shift-right shift-right",
    );
    let quads = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let shown = layout
                .prefix_shown(0)
                .first()
                .map(|piece| piece.0.to_owned());
            let quads = layout.selection_quads(&(at(3)..at(9)), &(px(0.)..layout.height()));
            let spans: Vec<_> = quads
                .iter()
                .map(|quad| (quad.bounds.left(), quad.bounds.right()))
                .collect();
            (shown, spans)
        })
        .unwrap();
    let (shown, spans) = quads;
    assert_eq!(shown.as_deref(), Some("- [ ] "));
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_x(spans[0].0, 3. * GLYPH);
    assert_x(spans[0].1, 9. * GLYPH);
}
