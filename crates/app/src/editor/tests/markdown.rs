use std::time::Instant;

use gpui::{TestAppContext, prelude::*};
use inkra_markdown::parse;
use inkra_text::{Buffer, ByteOffset};

use super::{dispatching, window_on, with_editor};
use crate::text_input::InputHost;

#[gpui::test]
fn a_note_is_styled_before_any_keystroke(cx: &mut TestAppContext) {
    with_editor(cx, "**b**", |view, _, _| {
        assert_eq!(view.spans(), parse("**b**").spans.as_slice());
    });
}

#[gpui::test]
fn typing_refreshes_the_spans_and_undo_restores_them(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "*b");
    let before = parse("*b").spans;

    cx.simulate_input("*");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.input.buffer().read(cx).text(), "*b*");
            assert_eq!(view.spans(), parse("*b*").spans.as_slice());
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.spans(), before.as_slice());
        })
        .unwrap();
}

#[gpui::test]
fn opening_another_note_replaces_the_spans(cx: &mut TestAppContext) {
    let other = cx.update(|cx| cx.new(|_| Buffer::new("# T")));
    with_editor(cx, "**b**", move |view, _, cx| {
        view.open_buffer(other, None, cx);
        assert_eq!(view.spans(), parse("# T").spans.as_slice());
    });
}

#[gpui::test]
fn a_write_outside_the_gates_refreshes_the_spans(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("b")));
    let window = window_on(cx, buffer.clone());

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(ByteOffset::new(1), "**");
            buffer.insert(ByteOffset::new(0), "**");
        });
        cx.notify();
    });
    cx.run_until_parked();

    window
        .update(cx, |view, _, _| {
            assert_eq!(view.spans(), parse("**b**").spans.as_slice());
        })
        .unwrap();
}

#[gpui::test]
fn an_edit_is_styled_before_effects_flush(cx: &mut TestAppContext) {
    with_editor(cx, "*b", |view, window, cx| {
        view.edit_buffer(window, cx, |_, buffer| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset::new(2), "*");
            });
        });
        // Inside the same update, so no observer has run: only `reshape` can have refreshed.
        assert_eq!(view.spans(), parse("*b*").spans.as_slice());
    });
}
