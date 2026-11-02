use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle, point, px};

use super::{at, dispatching};
use crate::editor::{EditorView, LinkFollowed};
use crate::open::Open;

const TEXT: &str = "go [[target]] end\nplain";

/// Each followed link's target and where it opens.
type Followed = Rc<RefCell<Vec<(String, Open)>>>;

/// Every `LinkFollowed` target, in order; the subscription lives as long as the returned guard.
fn followed(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
) -> (Followed, gpui::Subscription) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let subscription = window
        .update(cx, |_, _, cx| {
            let view = cx.entity();
            cx.subscribe(&view, move |_, _, event: &LinkFollowed, _| {
                sink.borrow_mut()
                    .push((event.link.target.clone(), event.open));
            })
        })
        .unwrap();
    (seen, subscription)
}

/// A point inside the glyph drawn for `offset`, in window coordinates.
fn point_at(
    window: &WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> gpui::Point<gpui::Pixels> {
    window
        .update(cx, |view, _, _| {
            let origin = view.input.last_bounds().unwrap().origin;
            let at = view
                .input
                .last_layout()
                .unwrap()
                .position_for_offset(at(offset));
            origin + at + point(px(2.), px(8.))
        })
        .unwrap()
}

fn caret(window: &WindowHandle<EditorView>, cx: &mut VisualTestContext) -> usize {
    window.update(cx, |view, _, _| view.cursor().get()).unwrap()
}

#[gpui::test]
fn cmd_enter_inside_a_link_follows_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    cx.simulate_keystrokes("cmd-up right right right right right");

    cx.simulate_keystrokes("cmd-enter");

    assert_eq!(*seen.borrow(), [("target".to_owned(), Open::Here)]);
    assert_eq!(caret(&window, &mut cx), 5, "following moves no caret");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.input.buffer().read(cx).text(), TEXT);
        })
        .unwrap();
}

#[gpui::test]
fn cmd_shift_enter_inside_a_link_follows_it_in_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    cx.simulate_keystrokes("cmd-up right right right right right");

    cx.simulate_keystrokes("cmd-shift-enter");

    assert_eq!(*seen.borrow(), [("target".to_owned(), Open::NewTab)]);
    assert_eq!(caret(&window, &mut cx), 5, "following moves no caret");
}

#[gpui::test]
fn cmd_enter_outside_a_link_does_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);

    cx.simulate_keystrokes("cmd-enter");

    assert!(seen.borrow().is_empty());
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.input.buffer().read(cx).text(), TEXT, "no newline");
        })
        .unwrap();
}

#[gpui::test]
fn a_cmd_click_on_a_concealed_link_follows_it_and_leaves_the_caret(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let before = caret(&window, &mut cx);
    let on_link = point_at(&window, &mut cx, 7);

    cx.simulate_mouse_down(on_link, MouseButton::Left, Modifiers::command());
    cx.run_until_parked();

    assert_eq!(*seen.borrow(), [("target".to_owned(), Open::Here)]);
    assert_eq!(caret(&window, &mut cx), before);
    window
        .update(&mut cx, |view, _, _| {
            assert!(
                !view.is_selecting,
                "no drag begins while the button is down"
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_cmd_shift_click_on_a_link_follows_it_in_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let before = caret(&window, &mut cx);
    let on_link = point_at(&window, &mut cx, 7);

    cx.simulate_mouse_down(on_link, MouseButton::Left, Modifiers::command_shift());
    cx.run_until_parked();

    assert_eq!(*seen.borrow(), [("target".to_owned(), Open::NewTab)]);
    assert_eq!(caret(&window, &mut cx), before, "no selection is extended");
}

#[gpui::test]
fn a_plain_click_on_a_link_places_the_caret(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let on_link = point_at(&window, &mut cx, 7);

    cx.simulate_click(on_link, Modifiers::none());
    cx.run_until_parked();

    assert!(seen.borrow().is_empty());
    let placed = caret(&window, &mut cx);
    assert!(
        (3..=13).contains(&placed),
        "caret at {placed}, inside the link"
    );
}

#[gpui::test]
fn a_cmd_click_off_a_link_is_a_plain_click(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, TEXT);
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let on_plain = point_at(&window, &mut cx, 19);

    cx.simulate_click(on_plain, Modifiers::command());
    cx.run_until_parked();

    assert!(seen.borrow().is_empty());
    assert_eq!(caret(&window, &mut cx), 19);
}

/// Past the end of a row, the click's offset is the link's end; nothing under the pointer is a link.
#[gpui::test]
fn a_cmd_click_past_a_line_ending_in_a_link_is_a_plain_click(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "see [[b]]\nnext");
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let past = point_at(&window, &mut cx, 9) + point(px(200.), px(0.));

    cx.simulate_click(past, Modifiers::command());
    cx.run_until_parked();

    assert!(seen.borrow().is_empty());
    assert_eq!(caret(&window, &mut cx), 9);
}

#[gpui::test]
fn a_cmd_click_below_the_text_is_a_plain_click(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "[[b]]");
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let below = point_at(&window, &mut cx, 2) + point(px(0.), px(300.));

    cx.simulate_click(below, Modifiers::command());
    cx.run_until_parked();

    assert!(seen.borrow().is_empty());
}

/// Concealed, `[[b]]` draws only `b`; the right half of that glyph rounds to the link's end
/// offset and still follows.
#[gpui::test]
fn a_cmd_click_on_a_links_last_drawn_glyph_follows_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "go [[b]] x\nnext");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let (left, right, top) = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let origin = view.input.last_bounds().unwrap().origin;
            (
                origin.x + layout.x_for_offset(at(5)),
                origin.x + layout.x_for_offset(at(6)),
                origin.y,
            )
        })
        .unwrap();
    assert!(right > left, "b is drawn");
    let right_half = point(left + (right - left) * 0.75, top + px(8.));

    cx.simulate_click(right_half, Modifiers::command());
    cx.run_until_parked();

    assert_eq!(*seen.borrow(), [("b".to_owned(), Open::Here)]);
}

#[gpui::test]
fn a_cmd_click_on_the_glyph_after_a_link_is_a_plain_click(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "go [[b]]xyz\nnext");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let (seen, _guard) = followed(&window, &mut cx);
    let after = point_at(&window, &mut cx, 8) - point(px(1.), px(0.)) + point(px(1.), px(0.));

    cx.simulate_click(after, Modifiers::command());
    cx.run_until_parked();

    assert!(seen.borrow().is_empty());
}
