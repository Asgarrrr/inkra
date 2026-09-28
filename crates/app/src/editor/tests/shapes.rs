//! A frame reuses the layout its inputs already produced (roadmap, "One shape per keystroke").

use std::ops::Range;

use gpui::{TestAppContext, TextStyle, prelude::*, px};
use inkra_text::{Buffer, ByteOffset};

use super::{at, dispatching, window_on};
use crate::editor::layout::shape_count;

#[gpui::test]
fn a_keystroke_shapes_the_note_once(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "# Title\n\nsome *text* and [[a link]]\n");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_input("x");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

#[gpui::test]
fn a_caret_move_that_reveals_nothing_shapes_nothing(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "plain words\n");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 0);
}

#[gpui::test]
fn a_write_outside_the_gates_reaches_the_next_paint(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("b")));
    let window = window_on(cx, buffer.clone());
    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(ByteOffset(1), "c");
        });
        cx.notify();
    });
    cx.run_until_parked();

    window
        .update(cx, |view, _, _| {
            assert_eq!(view.last_layout.as_ref().unwrap().shown(0), "bc");
        })
        .unwrap();
}

/// No gated path changes the style or reveals an element without reshaping, so only this test reaches them.
#[gpui::test]
fn a_layout_is_reused_only_for_inputs_that_shape_it_alike(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one *two* three\n");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.buffer.read(cx);
            let layout = view.last_layout.as_ref().unwrap();
            let (style, width) = (layout.style().clone(), layout.wrap_width());
            let reused = |selection: Range<ByteOffset>, marked: Option<Range<ByteOffset>>| {
                layout.is_shaped_from(
                    buffer,
                    view.markdown(),
                    &selection,
                    marked.as_ref(),
                    &style,
                    width,
                )
            };
            // `last_layout` still holds the end-of-document caret the window opened with.
            assert!(reused(at(0)..at(0), None), "revealing nothing, as before");
            assert!(
                !reused(at(6)..at(6), None),
                "inside `*two*`, which reveals it"
            );
            assert!(!reused(at(0)..at(0), Some(at(0)..at(3))), "a composition");

            let mut bigger = style.clone();
            bigger.font_size = px(40.).into();
            let other = |style: &TextStyle| {
                layout.is_shaped_from(buffer, view.markdown(), &(at(0)..at(0)), None, style, width)
            };
            assert!(!other(&bigger), "another style");
        })
        .unwrap();
}
