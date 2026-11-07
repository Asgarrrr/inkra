//! With `code_block_wrap` set, a code block line wraps at the column and nothing scrolls (code block
//! scroll spec, D7). Headless, a 120-glyph line is 1152 px against a 756 px column: two rows.

use gpui::{ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, WindowHandle, point, px};

use super::code_block_scroll::{block, swipe_block};
use super::{assert_x, at, close, dispatching, tall_document};
use crate::editor::EditorView;
use crate::editor::layout::painted_clips;
use crate::theme::set_code_block_wrap;

#[gpui::test]
fn a_wrapping_block_has_no_scroll(cx: &mut TestAppContext) {
    cx.update(|cx| set_code_block_wrap(true, cx));
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.block_backgrounds()[0].bottom(), 112.);
            assert_eq!(layout.row_count(), 5);
            assert!(layout.block_span(at(20)).is_none());
        })
        .unwrap();
    assert!(painted_clips().is_empty(), "a line was clipped");

    swipe_block(window, &mut cx, 0, -100.);

    window
        .update(&mut cx, |view, _, _| {
            assert_x(view.block_x(0), 0.);
            assert_eq!(view.viewport.handle().offset(), point(px(0.), px(0.)));
        })
        .unwrap();
    close(&mut cx);
}

fn scroll_y(window: WindowHandle<EditorView>, cx: &mut gpui::VisualTestContext) -> f32 {
    window
        .update(cx, |view, _, _| f32::from(view.scroll_y()))
        .unwrap()
}

#[gpui::test]
fn a_vertical_tick_over_a_wrapping_block_scrolls_the_page(cx: &mut TestAppContext) {
    cx.update(|cx| set_code_block_wrap(true, cx));
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let at = window
        .update(&mut cx, |view, _, _| view.block_center(0))
        .unwrap()
        .unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
        touch_phase: TouchPhase::Started,
        ..ScrollWheelEvent::default()
    });
    cx.run_until_parked();

    assert_x(px(scroll_y(window, &mut cx)), -30.);
    close(&mut cx);
}

#[gpui::test]
fn turning_wrap_off_again_restores_the_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -100.);

    for (wrap, expected) in [(true, 0.), (false, 100.)] {
        cx.update(|_, cx| set_code_block_wrap(wrap, cx));
        cx.run_until_parked();
        let x = window
            .update(&mut cx, |view, _, _| view.block_x(0))
            .unwrap();
        assert_x(x, expected);
    }
    close(&mut cx);
}
