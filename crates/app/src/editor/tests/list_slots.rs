//! List slots (task boxes spec, D3, D4, D11): nesting by level, one number width per list, a mark's
//! baseline, wrapped rows, the raw item part ending at the text start, and an item's other lines
//! starting at its text. Each `x` is compared with another line's measured `x` or with the slot's
//! constants.

use gpui::{Pixels, TestAppContext, VisualTestContext, WindowHandle, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::{painted_boxes, painted_marks};
use crate::theme::{CODE_BLOCK_OUTSET, LIST_SLOT_GAP, TASK_BOX_SIZE};

type Handle = WindowHandle<EditorView>;

/// A painted mark from the text's origin: its text, the middle and the right of its advance, and
/// its baseline.
type Mark = (String, Pixels, Pixels, Pixels);

fn slot() -> Pixels {
    TASK_BOX_SIZE + LIST_SLOT_GAP
}

/// The editor on `text` with the caret on its last line, off every item part.
fn editor(cx: &mut TestAppContext, text: &str) -> (Handle, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    (window, cx)
}

fn marks(window: &Handle, cx: &mut VisualTestContext) -> Vec<Mark> {
    cx.run_until_parked();
    let origin = window
        .update(cx, |view, _, _| view.text_bounds().unwrap().origin)
        .unwrap();
    painted_marks()
        .into_iter()
        .map(|mark| {
            let middle = (mark.left + mark.right) / 2. - origin.x;
            let baseline = mark.baseline - origin.y;
            (mark.text, middle, mark.right - origin.x, baseline)
        })
        .collect()
}

fn xs(window: &Handle, cx: &mut VisualTestContext, offsets: &[usize]) -> Vec<Pixels> {
    cx.run_until_parked();
    window
        .update(cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let x = |&offset| layout.x_for_offset(at(offset));
            offsets.iter().map(x).collect()
        })
        .unwrap()
}

/// What line `line`'s prefix paints: each piece's text, left and right.
fn pieces(
    window: &Handle,
    cx: &mut VisualTestContext,
    line: usize,
) -> Vec<(String, Pixels, Pixels)> {
    cx.run_until_parked();
    window
        .update(cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let pieces = layout.prefix_shown(line).into_iter();
            pieces
                .map(|(text, left, right)| (text.to_owned(), left, right))
                .collect()
        })
        .unwrap()
}

/// A `•` whose slot starts at `left`: the middle of its advance on the box's axis.
fn assert_bullet_at(mark: &Mark, left: Pixels) {
    assert_eq!(mark.0, "•");
    assert!(
        (mark.1 - (left + TASK_BOX_SIZE / 2.)).abs() < px(0.01),
        "{mark:?} centred at {left:?}"
    );
}

/// Each child's slot starts at its parent's text, whatever blanks nest it.
#[gpui::test]
fn a_child_sits_under_its_parents_text(app: &mut TestAppContext) {
    let (window, mut cx) = editor(app, "- a\n  - b\n    - c\n\nbody");
    let bullets = marks(&window, &mut cx);
    let text = xs(&window, &mut cx, &[2, 8, 16]);
    assert_eq!(bullets.len(), 3, "{bullets:?}");
    assert_bullet_at(&bullets[0], px(0.));
    assert_eq!(text[0], slot());
    assert_bullet_at(&bullets[1], text[0]);
    assert_eq!(text[1], text[0] + slot());
    assert_bullet_at(&bullets[2], text[1]);
    assert_eq!(text[2], text[1] + slot());
    for (note, b) in [("- a\n    - b\n\nbody", 10), ("- a\n\t- b\n\nbody", 7)] {
        let (window, mut cx) = editor(app, note);
        let nested = marks(&window, &mut cx);
        assert_bullet_at(&nested[1], text[0]);
        assert_eq!(xs(&window, &mut cx, &[b]), [text[1]], "{note:?}");
    }
}

/// An ordered task's child holds a blank slot for its number and one for its box.
#[gpui::test]
fn an_ordered_tasks_child_sits_past_its_number_and_box(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "1. [ ] a\n   - b\n\nbody");
    let marks = marks(&window, &mut cx);
    let text = xs(&window, &mut cx, &[7, 14]);
    assert_eq!(marks.len(), 2, "{marks:?}");
    assert_bullet_at(&marks[1], text[0]);
    assert_eq!(text[1], text[0] + slot());
}

/// `8.` to `10.` share one number width: their right edges line up, their texts start at one `x`,
/// and a child under `10.` starts at that text.
#[gpui::test]
fn one_list_shares_its_number_width(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "8. a\n9. b\n10. c\n    - d\n\nbody");
    let marks = marks(&window, &mut cx);
    let text = xs(&window, &mut cx, &[3, 8, 14, 22]);
    let numbers: Vec<_> = marks.iter().take(3).map(|mark| mark.0.as_str()).collect();
    assert_eq!(numbers, ["8.", "9.", "10."]);
    assert_eq!((text[1], text[2]), (text[0], text[0]));
    for mark in &marks[..3] {
        assert_x(mark.2, f32::from(text[0] - LIST_SLOT_GAP));
    }
    assert_bullet_at(&marks[3], text[2]);
    assert_eq!(text[3], text[2] + slot());
}

/// Headless at 18 px, `1.` and the gap pass a slot, so an ordered list's text starts right of a
/// bullet list's.
#[gpui::test]
fn a_number_wider_than_a_slot_widens_its_list(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "1. a\n\n- b\n\nbody");
    let text = xs(&window, &mut cx, &[3, 8]);
    assert_eq!(text[1], slot());
    assert!(text[0] > text[1], "{text:?}");
}

/// The `•` before a heading sits on the heading's first-row baseline.
#[gpui::test]
fn a_bullet_before_a_heading_sits_on_its_baseline(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- # a\n\nbody");
    let marks = marks(&window, &mut cx);
    let (top, row_height, ascent, descent) = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().row_metrics(0)
        })
        .unwrap();
    // gpui's baseline: half the leading, then the ascent (`text_system/line.rs:557-558`).
    let heading = top + (row_height - ascent - descent) / 2. + ascent;
    assert_eq!(marks.len(), 1, "{marks:?}");
    assert!(
        (marks[0].3 - heading).abs() < px(0.01),
        "{marks:?} {heading:?}"
    );
}

/// A wrapped bullet's later rows start at its text.
#[gpui::test]
fn a_wrapped_bullets_rows_start_at_its_text(cx: &mut TestAppContext) {
    let text = format!("- {}\nbody", "aaaa ".repeat(40));
    let (window, mut cx) = editor(cx, &text);
    cx.run_until_parked();
    let starts = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let last = layout.row_for_offset(at(2 + 200));
            let wrapped = (1..=last).map(|row| layout.offset_on_row(row, px(0.)));
            let starts = std::iter::once(at(2)).chain(wrapped);
            starts
                .map(|offset| layout.x_for_offset(offset))
                .collect::<Vec<_>>()
        })
        .unwrap();
    assert!(starts.len() > 2, "the line wraps: {starts:?}");
    assert!(starts.iter().all(|&x| x == slot()), "{starts:?}");
}

/// A caret at 0 shows the raw `- `, or `  - ` on a nested line, ending at the text start, and the
/// text keeps its `x`: a bullet's raw part is narrower than its slots.
#[gpui::test]
fn a_revealed_bullet_ends_at_its_text_which_stays(cx: &mut TestAppContext) {
    for (note, keys, line, raw, text_start) in [
        ("- a\nbody", "cmd-up", 0, "- ", 2),
        ("- a\n  - b\nbody", "cmd-up down home", 1, "  - ", 8),
    ] {
        let (window, mut cx) = editor(cx, note);
        let concealed = xs(&window, &mut cx, &[text_start]);
        cx.simulate_keystrokes(keys);
        let revealed = xs(&window, &mut cx, &[text_start]);
        let pieces = pieces(&window, &mut cx, line);
        assert_eq!(revealed, concealed, "{note:?}");
        assert_eq!(pieces.len(), 1, "{note:?}: {pieces:?}");
        let (text, _, right) = &pieces[0];
        assert_eq!((text.as_str(), *right), (raw, revealed[0]), "{note:?}");
        assert_eq!(
            marks(&window, &mut cx).len(),
            line,
            "{note:?}: line 0's `•` only"
        );
    }
}

/// A selection over both parts of `> - a` paints `> ` at 0 and `- ` ending at the text start.
#[gpui::test]
fn a_selection_over_both_parts_paints_two_pieces(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "> - a\nbody");
    let concealed = xs(&window, &mut cx, &[4]);
    cx.simulate_keystrokes("cmd-up shift-right shift-right shift-right");
    let pieces = pieces(&window, &mut cx, 0);
    assert_eq!(xs(&window, &mut cx, &[4]), concealed);
    let texts: Vec<_> = pieces.iter().map(|piece| piece.0.as_str()).collect();
    assert_eq!(texts, ["> ", "- "]);
    assert_eq!(pieces[0].1, px(0.));
    assert_eq!(pieces[1].2, concealed[0]);
    assert!(pieces[1].1 > pieces[0].2, "{pieces:?}");
}

/// A continuation line, a lazy line, a later paragraph and a line indented by a tab's width start
/// at the item's text, in a quote and out of one (D11).
#[gpui::test]
fn an_items_other_lines_start_at_its_text(cx: &mut TestAppContext) {
    for (note, a, b) in [
        ("- a\n  b\n\nbody", 2, 6),
        ("- a\nb\n\nbody", 2, 4),
        ("- a\n\n  b\n\nbody", 2, 7),
        ("-\ta\n    b\n\nbody", 2, 8),
        ("> - a\nb\n\nbody", 4, 6),
        ("- a\n  - b\n  c\n\nbody", 8, 12),
        ("8. a\n9. b\n10. c\n    d\n\nbody", 14, 20),
    ] {
        let (window, mut cx) = editor(cx, note);
        let text = xs(&window, &mut cx, &[a, b]);
        assert_eq!(text[1], text[0], "{note:?}");
    }
}

/// A quoted task's lazy line carries the quote part, takes a blank slot, and paints no box.
#[gpui::test]
fn a_quoted_tasks_lazy_line_starts_under_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "> - [ ] a\nb\n\nbody");
    let text = xs(&window, &mut cx, &[8, 10]);
    assert_eq!(text[1], text[0]);
    cx.run_until_parked();
    assert_eq!(painted_boxes().len(), 1);
}

/// A fenced block in an item starts its background at the item's text, and a caret on its empty
/// line sits there, inside the background.
#[gpui::test]
fn a_code_block_in_an_item_starts_at_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- a\n\n  ```\n  x\n\n  ```\n\nbody");
    let text = xs(&window, &mut cx, &[2, 13, 15]);
    let background = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().block_backgrounds()
        })
        .unwrap();
    assert_eq!(background.len(), 1, "{background:?}");
    assert_eq!(background[0].left(), text[0] - CODE_BLOCK_OUTSET);
    assert_eq!((text[1], text[2]), (text[0], text[0]));
}

/// Home on a continuation line reveals its blanks, right-aligned against the text, and a letter
/// typed there lands before them.
#[gpui::test]
fn home_on_a_continuation_line_lands_left_of_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- a\n  b\n\nbody");
    let concealed = xs(&window, &mut cx, &[6]);
    cx.simulate_keystrokes("cmd-up down home");
    let caret = window.update(&mut cx, |view, _, _| view.cursor().get());
    assert_eq!(caret.unwrap(), 4);
    let revealed = xs(&window, &mut cx, &[4, 6]);
    assert_eq!(revealed[1], concealed[0]);
    assert!(revealed[0] < revealed[1], "{revealed:?}");
    cx.simulate_keystrokes("x");
    let text = window.update(&mut cx, |view, _, cx| {
        view.buffer().read(cx).text().to_owned()
    });
    assert_eq!(text.unwrap(), "- a\nx  b\n\nbody");
}

/// A wrapped continuation line's rows all start at the item's text.
#[gpui::test]
fn a_wrapped_continuation_lines_rows_start_at_its_text(cx: &mut TestAppContext) {
    let text = format!("- a\n  {}\nbody", "aaaa ".repeat(40));
    let (window, mut cx) = editor(cx, &text);
    cx.run_until_parked();
    let (a, starts) = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let first = layout.row_for_offset(at(6));
            let last = layout.row_for_offset(at(6 + 200));
            let wrapped = (first + 1..=last).map(|row| layout.offset_on_row(row, px(0.)));
            let starts = std::iter::once(at(6)).chain(wrapped);
            let starts: Vec<_> = starts.map(|offset| layout.x_for_offset(offset)).collect();
            (layout.x_for_offset(at(2)), starts)
        })
        .unwrap();
    assert!(starts.len() > 2, "the line wraps: {starts:?}");
    assert!(starts.iter().all(|&x| x == a), "{a:?}: {starts:?}");
}
