//! A click on a task's box toggles it (task boxes spec, D4's hit test, D5). Every click lands on a
//! box as the last paint drew it, in window coordinates.

use gpui::{
    Bounds, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext, WindowHandle, point, px,
};

use super::geometry::LINE_HEIGHT;
use super::{at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::painted_boxes;

type Handle = WindowHandle<EditorView>;

/// Each box the last paint drew, in window coordinates.
fn boxes(cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    cx.run_until_parked();
    painted_boxes()
        .into_iter()
        .map(|task| task.bounds)
        .collect()
}

/// The only box painted.
fn only_box(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    let boxes = boxes(cx);
    assert_eq!(boxes.len(), 1, "{boxes:?}");
    boxes[0]
}

fn text(window: &Handle, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

fn caret(window: &Handle, cx: &mut VisualTestContext) -> usize {
    window.update(cx, |view, _, _| view.cursor().get()).unwrap()
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_click(at, Modifiers::none());
}

/// A click on the only box's centre.
fn click_box(cx: &mut VisualTestContext) {
    let centre = only_box(cx).center();
    click(cx, centre);
}

/// The editor on `text` with the caret on its last line, off every task part.
fn editor(cx: &mut TestAppContext, text: &str) -> (Handle, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    (window, cx)
}

#[gpui::test]
fn a_click_on_a_box_toggles_it_and_puts_the_caret_on_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    click_box(&mut cx);
    assert_eq!(text(&window, &mut cx), "- [x] a\nbody");
    assert_eq!(caret(&window, &mut cx), 6);

    click_box(&mut cx);
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody");
    assert_eq!(caret(&window, &mut cx), 6);
}

/// The caret lands at the text start the layout holds, where the parse ends the gutter: past one
/// space or tab, and at the line's end with no text.
#[gpui::test]
fn a_toggle_puts_the_caret_at_the_gutter_end(cx: &mut TestAppContext) {
    for note in [
        "- [ ]\t\ta\nbody",
        "- [ ]  a\nbody",
        "- [ ]\n\nbody",
        "- [ ] \n\nbody",
    ] {
        let (window, mut cx) = editor(cx, note);
        click_box(&mut cx);
        let toggled = text(&window, &mut cx);
        let gutter = inkra_markdown::parse(&toggled).gutters[0].range.end;
        assert_eq!(caret(&window, &mut cx), gutter.get(), "{note:?}");
    }
}

#[gpui::test]
fn a_click_clears_an_upper_case_tick(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [X] a\nbody");
    click_box(&mut cx);
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody");
}

#[gpui::test]
fn undo_restores_the_text_a_click_toggled(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    click_box(&mut cx);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody");
}

#[gpui::test]
fn a_quoted_task_toggles(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "> - [ ] a\nbody");
    click_box(&mut cx);
    assert_eq!(text(&window, &mut cx), "> - [x] a\nbody");
    assert_eq!(caret(&window, &mut cx), 8);
}

/// A revealed checkbox is text: a click on it places the caret there.
#[gpui::test]
fn a_click_on_a_revealed_checkbox_places_the_caret_and_edits_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    let centre = only_box(&mut cx).center();
    cx.simulate_keystrokes("cmd-up");
    assert_eq!(boxes(&mut cx), [], "the caret at 0 reveals the task part");
    let placed = window
        .update(&mut cx, |view, _, _| view.input.offset_at(centre).unwrap())
        .unwrap();
    assert!((at(2)..at(5)).contains(&placed), "{placed:?}");

    click(&mut cx, centre);
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody");
    assert_eq!(caret(&window, &mut cx), placed.get());
}

#[gpui::test]
fn a_press_on_a_box_then_a_drag_selects_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    let centre = only_box(&mut cx).center();
    let below = centre + point(px(40.), px(LINE_HEIGHT));

    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(text(&window, &mut cx), "- [x] a\nbody");
    let selected = window
        .update(&mut cx, |view, _, _| view.selected())
        .unwrap();
    assert_eq!(selected, at(6)..at(6));
}

/// `⌘` keeps following links and `⇧` keeps extending the selection.
#[gpui::test]
fn a_command_or_shift_click_on_a_box_does_not_toggle(cx: &mut TestAppContext) {
    for modifiers in [Modifiers::command(), Modifiers::shift()] {
        let (window, mut cx) = editor(cx, "- [ ] a\nbody");
        let centre = only_box(&mut cx).center();
        cx.simulate_click(centre, modifiers);
        assert_eq!(text(&window, &mut cx), "- [ ] a\nbody", "{modifiers:?}");
    }
}

/// The click that activates an inactive window only activates it, as Electron's default does.
#[gpui::test]
fn a_window_activating_click_on_a_box_does_not_toggle(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    let centre = only_box(&mut cx).center();
    window
        .update(&mut cx, |view, window, cx| {
            let press = MouseDownEvent {
                position: centre,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: true,
            };
            view.on_mouse_down(&press, window, cx);
        })
        .unwrap();
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody");
}

/// A point left of, right of or below the square, still on the line's first row.
#[gpui::test]
fn a_click_just_outside_the_box_does_not_toggle(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    let square = only_box(&mut cx);
    let (centre, one) = (square.center(), px(1.));
    for outside in [
        point(square.left() - one, centre.y),
        point(square.right() + one, centre.y),
        point(centre.x, square.bottom() + one),
    ] {
        assert!(
            outside.y < square.center().y + px(LINE_HEIGHT / 2.),
            "on row 0"
        );
        assert_eq!(only_box(&mut cx), square);
        click(&mut cx, outside);
        assert_eq!(text(&window, &mut cx), "- [ ] a\nbody", "{outside:?}");
        cx.simulate_keystrokes("cmd-down");
    }
}

/// A click below the document lands on its last line; only that line's first row holds the box.
#[gpui::test]
fn a_click_below_the_last_line_does_not_toggle_its_box(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a");
    let below = only_box(&mut cx).center() + point(px(0.), px(LINE_HEIGHT));
    click(&mut cx, below);
    assert_eq!(text(&window, &mut cx), "- [ ] a");
}

/// The lazy line under a quoted task carries no box to hit (spec D3, D).
#[gpui::test]
fn a_lazy_line_has_no_box_to_hit(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "> - [ ] a\n  b\n\nbody");
    let lazy = only_box(&mut cx).center() + point(px(0.), px(LINE_HEIGHT));
    click(&mut cx, lazy);
    assert_eq!(text(&window, &mut cx), "> - [ ] a\n  b\n\nbody");
}

/// After a change the layout has not caught up with, three matching bytes do not prove the
/// task is the same one (spec D5, G).
#[gpui::test]
fn a_click_on_a_layout_older_than_the_buffer_edits_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- [ ] a\nbody");
    let centre = only_box(&mut cx).center();
    window
        .update(&mut cx, |view, window, cx| {
            let buffer = view.buffer().clone();
            let end = buffer.read(cx).len();
            buffer.update(cx, |buffer, _| {
                buffer.transact(|txn| txn.replace(end..end, "!"));
            });
            let press = MouseDownEvent {
                position: centre,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            };
            view.on_mouse_down(&press, window, cx);
        })
        .unwrap();
    assert_eq!(text(&window, &mut cx), "- [ ] a\nbody!");
}

/// The caret lands on the clicked line, so a toggle far below the old caret keeps the view.
#[gpui::test]
fn a_toggle_away_from_the_old_caret_does_not_scroll(cx: &mut TestAppContext) {
    let note = format!("{}- [ ] a\n{}", "line\n".repeat(100), "line\n".repeat(100));
    let (window, mut cx) = dispatching(cx, &note);
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, cx| view.scroll_to_top(at(90 * 5), cx))
        .unwrap();
    let centre = only_box(&mut cx).center();
    let scrolled = window
        .update(&mut cx, |view, _, _| view.scroll_y())
        .unwrap();
    assert!(scrolled < px(0.), "scrolled away from the caret at 0");

    click(&mut cx, centre);
    cx.run_until_parked();
    assert!(text(&window, &mut cx).contains("- [x] a"));
    let after = window
        .update(&mut cx, |view, _, _| view.scroll_y())
        .unwrap();
    assert_eq!(after, scrolled);
}
