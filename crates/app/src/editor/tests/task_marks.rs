//! The marks before the box (task boxes spec, D3, D10, D11): a concealed item part paints each mark
//! in its slot, then the box. Headless, a body glyph is 10.8 px, so `1.` and the gap pass one slot.

use gpui::{Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, point, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::{painted_boxes, painted_marks};
use crate::theme::{LIST_SLOT_GAP, TASK_BOX_SIZE};

type Handle = WindowHandle<EditorView>;

/// A painted task's box left edge and whether it is checked.
type Painted = (Pixels, bool);

fn origin(window: &Handle, cx: &mut VisualTestContext) -> gpui::Point<Pixels> {
    cx.run_until_parked();
    window
        .update(cx, |view, _, _| view.text_bounds().unwrap().origin)
        .unwrap()
}

/// Each box the last paint drew, from the text's origin, and whether it is checked.
fn painted(window: &Handle, cx: &mut VisualTestContext) -> Vec<Painted> {
    let origin = origin(window, cx);
    painted_boxes()
        .into_iter()
        .map(|task| (task.bounds.left() - origin.x, task.checked))
        .collect()
}

/// Each mark the last paint drew: its text and its left and right edges from the text's origin.
fn marks(window: &Handle, cx: &mut VisualTestContext) -> Vec<(String, Pixels, Pixels)> {
    let origin = origin(window, cx);
    painted_marks()
        .into_iter()
        .map(|mark| (mark.text, mark.left - origin.x, mark.right - origin.x))
        .collect()
}

/// The one task painted: its box's left edge and its state.
fn only(window: &Handle, cx: &mut VisualTestContext) -> Painted {
    let painted = painted(window, cx);
    assert_eq!(painted.len(), 1, "{painted:?}");
    painted[0]
}

fn text(window: &Handle, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

fn x(window: &Handle, cx: &mut VisualTestContext, offset: usize) -> Pixels {
    window
        .update(cx, |view, _, _| {
            view.input.last_layout().unwrap().x_for_offset(at(offset))
        })
        .unwrap()
}

/// The editor on `text` with the caret on its last line, off every item part.
fn editor(cx: &mut TestAppContext, text: &str) -> (Handle, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    (window, cx)
}

fn slot() -> Pixels {
    TASK_BOX_SIZE + LIST_SLOT_GAP
}

/// A number ends a gap left of its slot's end, where the box's slot starts.
#[gpui::test]
fn an_ordered_task_paints_its_number_then_its_box(cx: &mut TestAppContext) {
    for (note, number, text_start, checked) in [
        ("1. [ ] a\nbody", "1.", 7, false),
        ("12) [x] a\nbody", "12)", 8, true),
    ] {
        let (window, mut cx) = editor(cx, note);
        let (left, painted) = only(&window, &mut cx);
        let marks = marks(&window, &mut cx);
        assert_eq!(painted, checked, "{note:?}");
        assert_eq!(marks.len(), 1, "{note:?}: {marks:?}");
        assert_eq!(marks[0].0, number);
        assert_x(marks[0].2, f32::from(left - LIST_SLOT_GAP));
        assert!(marks[0].1 >= px(0.), "{note:?}: {marks:?}");
        assert_x(x(&window, &mut cx, text_start), f32::from(left + slot()));
    }
}

/// `- - [ ] a` paints a `•` in slot 0 and the box in slot 1; `- 1. [ ] a` a `•`, its number, then
/// the box.
#[gpui::test]
fn a_task_after_a_bullet_paints_a_dot_then_its_box(app: &mut TestAppContext) {
    let (window, mut cx) = editor(app, "- - [ ] a\nbody");
    let painted_marks = marks(&window, &mut cx);
    assert_eq!(only(&window, &mut cx), (slot(), false));
    assert_eq!(painted_marks.len(), 1, "{painted_marks:?}");
    assert_eq!(painted_marks[0].0, "•");
    assert_x(
        (painted_marks[0].1 + painted_marks[0].2) / 2.,
        f32::from(TASK_BOX_SIZE / 2.),
    );

    let (window, mut cx) = editor(app, "1. - [x] a\nbody");
    let (left, checked) = only(&window, &mut cx);
    let painted_marks = marks(&window, &mut cx);
    assert!(checked);
    assert_eq!(
        painted_marks
            .iter()
            .map(|mark| mark.0.as_str())
            .collect::<Vec<_>>(),
        ["1."]
    );
    assert_x(painted_marks[0].2, f32::from(left - LIST_SLOT_GAP));

    let (window, mut cx) = editor(app, "- 1. [ ] a\nbody");
    let (left, _) = only(&window, &mut cx);
    let painted_marks = marks(&window, &mut cx);
    let texts: Vec<_> = painted_marks.iter().map(|mark| mark.0.as_str()).collect();
    assert_eq!(texts, ["•", "1."]);
    assert_x(painted_marks[1].2, f32::from(left - LIST_SLOT_GAP));
    assert!(painted_marks[1].1 >= slot(), "{painted_marks:?}");
}

/// The marks start at the item part, right of the quote part, where the quoted line's text does.
#[gpui::test]
fn quoted_marks_start_at_the_quote_parts_width(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "> - - [ ] a\n>\n> b\nbody");
    let quote = x(&window, &mut cx, 16);
    let marks = marks(&window, &mut cx);
    let (left, checked) = only(&window, &mut cx);
    assert!(!checked);
    assert_x(left, f32::from(quote + slot()));
    assert_x(
        (marks[0].1 + marks[0].2) / 2.,
        f32::from(quote + TASK_BOX_SIZE / 2.),
    );
}

/// A line whose marks all stand on earlier lines paints only its box after its blank slots.
#[gpui::test]
fn blank_slots_paint_nothing(app: &mut TestAppContext) {
    let (window, mut cx) = editor(app, "- [ ] a\nbody");
    assert_eq!(
        (only(&window, &mut cx), marks(&window, &mut cx)),
        ((px(0.), false), vec![])
    );
    let (window, mut cx) = editor(app, "- a\n  - [ ] b\nbody");
    let painted_marks = marks(&window, &mut cx);
    assert_eq!(only(&window, &mut cx), (x(&window, &mut cx, 2), false));
    assert_eq!(
        painted_marks.len(),
        1,
        "line 0's `•` alone: {painted_marks:?}"
    );
}

/// A caret on the number reveals the whole item part raw, so neither the marks nor the box paint.
#[gpui::test]
fn a_caret_on_the_marks_reveals_the_item_part_raw(cx: &mut TestAppContext) {
    for (note, keys, raw) in [
        ("1. [ ] a\nbody", "cmd-up right", "1. [ ] "),
        ("- - [ ] a\nbody", "cmd-up", "- - [ ] "),
        ("- - [ ] a\nbody", "cmd-up right right", "- - [ ] "),
    ] {
        let (window, mut cx) = editor(cx, note);
        cx.simulate_keystrokes(keys);
        assert_eq!(painted(&window, &mut cx), [], "{note:?} after {keys}");
        assert_eq!(marks(&window, &mut cx), [], "{note:?} after {keys}");
        let shown = window
            .update(&mut cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                let shown = layout.prefix_shown(0).into_iter();
                shown
                    .map(|(text, left, _)| (text.to_owned(), left))
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(shown, [(raw.to_owned(), px(0.))], "{note:?} after {keys}");
    }
}

/// `⌘L` ticks the number's task: the text keeps its `x`, the number and the box their place.
#[gpui::test]
fn toggling_an_ordered_task_keeps_its_marks_and_width(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "1. [ ] a\nbody");
    let x = |cx: &mut VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                view.input.last_layout().unwrap().x_for_offset(at(7))
            })
            .unwrap()
    };
    let (unchecked_x, (left, _)) = (x(&mut cx), only(&window, &mut cx));
    let number = marks(&window, &mut cx);
    cx.simulate_keystrokes("cmd-up cmd-l cmd-down");
    assert_eq!(text(&window, &mut cx), "1. [x] a\nbody");
    assert_eq!(x(&mut cx), unchecked_x);
    assert_eq!(only(&window, &mut cx), (left, true));
    assert_eq!(marks(&window, &mut cx), number);
}

/// A click on a `•` or a number lands as on any concealed slot, at the text start, and edits
/// nothing (D4).
#[gpui::test]
fn a_click_on_a_mark_places_the_caret_at_the_text_start(cx: &mut TestAppContext) {
    for (note, text_start) in [("1. [ ] a\nbody", 7), ("1. a\nbody", 3), ("- a\nbody", 2)] {
        let (window, mut cx) = editor(cx, note);
        let origin = origin(&window, &mut cx);
        let marks = marks(&window, &mut cx);
        let (_, left, right) = &marks[0];
        let on = origin + point((*left + *right) / 2., px(10.));
        cx.simulate_click(on, Modifiers::none());
        let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
        assert_eq!(
            (caret, text(&window, &mut cx)),
            (at(text_start), note.to_owned())
        );
    }
}

/// `- > - [ ] a`: the quote inside the item has no gutter, so its task has no box, and `⌘L`
/// leaves it rather than add a second checkbox.
#[gpui::test]
fn a_task_behind_a_quote_in_an_item_paints_no_box_and_cmd_l_leaves_it(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- > - [ ] a\nbody");
    cx.simulate_keystrokes("cmd-up");
    assert_eq!(painted(&window, &mut cx), []);
    cx.simulate_keystrokes("cmd-l");
    assert_eq!(text(&window, &mut cx), "- > - [ ] a\nbody");
}
