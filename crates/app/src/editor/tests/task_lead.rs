//! The markers before the box (task boxes spec, D10): a concealed task part paints its lead through
//! the lead's own conceals, then the box. Headless, a body glyph is 10.8 px.

use gpui::{Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, point, px};

use super::{assert_x, at, dispatching};
use crate::editor::EditorView;
use crate::editor::layout::painted_boxes;

const GLYPH: f32 = 0.6 * 18.;

type Handle = WindowHandle<EditorView>;

/// A painted task's box centre, whether it is checked, and its lead's text and `x`.
type Painted = (Pixels, bool, Option<(String, Pixels)>);

/// Each task the last paint drew, from the text's origin: its box's centre `x`, whether it is
/// checked, and the lead painted before it.
fn painted(window: &Handle, cx: &mut VisualTestContext) -> Vec<Painted> {
    cx.run_until_parked();
    let origin = window
        .update(cx, |view, _, _| view.text_bounds().unwrap().origin)
        .unwrap();
    painted_boxes()
        .into_iter()
        .map(|task| {
            let lead = task.lead.map(|(text, x)| (text, x - origin.x));
            (task.bounds.center().x - origin.x, task.checked, lead)
        })
        .collect()
}

/// The one task painted: its box's centre, its state, and its lead's text and `x`.
fn only(window: &Handle, cx: &mut VisualTestContext) -> Painted {
    let painted = painted(window, cx);
    assert_eq!(painted.len(), 1, "{painted:?}");
    painted.into_iter().next().unwrap()
}

fn text(window: &Handle, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

/// The editor on `text` with the caret on its last line, off every task part.
fn editor(cx: &mut TestAppContext, text: &str) -> (Handle, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    (window, cx)
}

/// `note`'s one task paints `lead` at `lead_x` glyphs, and its box centred at `centre` glyphs.
fn assert_task(
    cx: &mut TestAppContext,
    note: &str,
    lead: Option<(&str, f32)>,
    centre: f32,
    checked: bool,
) {
    let (window, mut cx) = editor(cx, note);
    let (x, painted, painted_lead) = only(&window, &mut cx);
    assert_x(x, centre * GLYPH);
    assert_eq!(painted, checked, "{note:?}");
    let painted_lead = painted_lead.map(|(text, x)| (text, (x / px(GLYPH)).round()));
    let lead = lead.map(|(text, x)| (text.to_owned(), x));
    assert_eq!(painted_lead, lead, "{note:?}");
}

#[gpui::test]
fn an_ordered_task_paints_its_number_then_its_box(cx: &mut TestAppContext) {
    assert_task(cx, "1. [ ] a\nbody", Some(("1. ", 0.)), 4.5, false);
    assert_task(cx, "12) [x] a\nbody", Some(("12) ", 0.)), 5.5, true);
}

#[gpui::test]
fn a_task_after_a_bullet_paints_a_dot_then_its_box(cx: &mut TestAppContext) {
    assert_task(cx, "- - [ ] a\nbody", Some(("• ", 0.)), 5.5, false);
    assert_task(cx, "1. - [x] a\nbody", Some(("1. ", 0.)), 6.5, true);
    assert_task(cx, "- 1. [ ] a\nbody", Some(("• 1. ", 0.)), 6.5, false);
}

/// The lead starts at the task part, right of the quote part.
#[gpui::test]
fn a_quoted_lead_paints_at_the_task_parts_x(cx: &mut TestAppContext) {
    assert_task(cx, "> 1. [ ] a\nbody", Some(("1. ", 2.)), 6.5, false);
    assert_task(cx, "> - - [ ] a\nbody", Some(("• ", 2.)), 7.5, false);
}

/// A lead of blanks, or none, shapes nothing.
#[gpui::test]
fn a_blank_lead_paints_nothing(cx: &mut TestAppContext) {
    assert_task(cx, "- [ ] a\nbody", None, 3.5, false);
    assert_task(cx, "- a\n  - [ ] b\nbody", None, 5.5, false);
}

/// A caret on the number reveals the whole task part raw, so neither the lead nor the box paints.
#[gpui::test]
fn a_caret_in_the_lead_reveals_the_task_part_raw(cx: &mut TestAppContext) {
    for (note, keys, raw) in [
        ("1. [ ] a\nbody", "cmd-up right", "1. [ ] "),
        ("- - [ ] a\nbody", "cmd-up", "- - [ ] "),
        ("- - [ ] a\nbody", "cmd-up right right", "- - [ ] "),
    ] {
        let (window, mut cx) = editor(cx, note);
        cx.simulate_keystrokes(keys);
        assert_eq!(painted(&window, &mut cx), [], "{note:?} after {keys}");
        let shown = window
            .update(&mut cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                layout.prefix_shown(0).map(|(text, x)| (text.to_owned(), x))
            })
            .unwrap();
        assert_eq!(
            shown,
            Some((raw.to_owned(), px(0.))),
            "{note:?} after {keys}"
        );
    }
}

/// `⌘L` ticks the number's task: the text keeps its `x`, the lead and the box their place.
#[gpui::test]
fn toggling_an_ordered_task_keeps_its_lead_and_width(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "1. [ ] a\nbody");
    let x = |cx: &mut VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                view.input.last_layout().unwrap().x_for_offset(at(7))
            })
            .unwrap()
    };
    let (unchecked_x, (centre, _, lead)) = (x(&mut cx), only(&window, &mut cx));
    cx.simulate_keystrokes("cmd-up cmd-l cmd-down");
    assert_eq!(text(&window, &mut cx), "1. [x] a\nbody");
    assert_eq!(x(&mut cx), unchecked_x);
    assert_eq!(only(&window, &mut cx), (centre, true, lead));
}

/// A click on the lead lands as on a concealed part, at the text start, and toggles nothing.
#[gpui::test]
fn a_click_on_the_lead_places_the_caret_at_the_text_start(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "1. [ ] a\nbody");
    let origin = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap().origin)
        .unwrap();
    cx.simulate_click(origin + point(px(0.5 * GLYPH), px(10.)), Modifiers::none());
    let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(
        (caret, text(&window, &mut cx)),
        (at(7), "1. [ ] a\nbody".to_owned())
    );
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
