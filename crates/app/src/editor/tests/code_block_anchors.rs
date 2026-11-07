//! A block keeps its offset across edits, rebinds, restores and parking (code block scroll spec, D1).

use std::panic::{AssertUnwindSafe, catch_unwind};

use gpui::{
    Entity, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext,
    WindowHandle, point, px,
};
use inkra_editor::Selection;
use inkra_text::{Anchor, Buffer, ByteOffset};

use super::code_block_scroll::{block, swipe_block};
use super::{assert_x, at, close, dispatching};
use crate::editor::{EditorView, ParkedView, TopLine, ViewState};
use crate::restore::SavedView;
use crate::text_input::InputHost;

fn swiped(cx: &mut TestAppContext, x: f32) -> (WindowHandle<EditorView>, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, x);
    (window, cx)
}

fn insert_above(window: WindowHandle<EditorView>, cx: &mut VisualTestContext, text: &str) {
    window
        .update(cx, |view, window, cx| {
            view.edit_buffer(window, cx, |_, buffer| {
                buffer.transact(|buffer| buffer.insert(at(0), text));
            });
            view.refresh(cx);
        })
        .unwrap();
    cx.run_until_parked();
}

fn anchors(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Vec<Anchor> {
    window
        .update(cx, |view, _, _| view.scrolls.anchors())
        .unwrap()
}

fn is_live(buffer: &Entity<Buffer>, anchor: Anchor, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        let buffer = buffer.read(cx);
        catch_unwind(AssertUnwindSafe(|| buffer.resolve(anchor))).is_ok()
    })
}

fn buffer_of(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Entity<Buffer> {
    window
        .update(cx, |view, _, _| view.buffer().clone())
        .unwrap()
}

#[gpui::test]
fn an_edit_above_keeps_the_offset(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);

    insert_above(window, &mut cx, "abc\n");

    window
        .update(&mut cx, |view, _, _| {
            assert_x(view.block_x(0), 100.);
            assert_eq!(view.scrolls.entries(), [(at(4), px(100.))]);
        })
        .unwrap();
    close(&mut cx);
}

#[gpui::test]
fn a_fence_typed_above_drops_the_entry(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);
    let old = anchors(window, &mut cx)[0];

    insert_above(window, &mut cx, "```\n");

    assert!(anchors(window, &mut cx).is_empty());
    let buffer = buffer_of(window, &mut cx);
    assert!(!is_live(&buffer, old, &mut cx));
    window
        .update(&mut cx, |view, _, _| {
            for index in 0..2 {
                assert_x(view.block_x(index), 0.);
            }
        })
        .unwrap();
}

#[gpui::test]
fn zero_and_back_reuses_the_anchor(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);
    let first = anchors(window, &mut cx);
    assert_eq!(first.len(), 1);

    swipe_block(window, &mut cx, 0, 100.);
    assert_eq!(anchors(window, &mut cx), first);
    window
        .update(&mut cx, |view, _, _| {
            assert_x(view.block_x(0), 0.);
            assert!(view.view_state().scrolls.is_empty());
        })
        .unwrap();

    swipe_block(window, &mut cx, 0, -50.);
    assert_eq!(anchors(window, &mut cx), first);
    window
        .update(&mut cx, |view, _, _| assert_x(view.block_x(0), 50.))
        .unwrap();
    close(&mut cx);
}

/// The hitbox comes from the frame drawn before the edit: no block starts at its offset now.
#[gpui::test]
fn a_hitbox_drawn_before_an_edit_takes_no_wheel(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);
    let event = ScrollWheelEvent {
        delta: ScrollDelta::Pixels(point(px(-50.), px(0.))),
        touch_phase: TouchPhase::Started,
        ..ScrollWheelEvent::default()
    };

    window
        .update(&mut cx, |view, _, cx| {
            view.buffer().update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.insert(at(0), "abc\n"));
            });
            assert!(!view.on_wheel(&event, Some((at(0), px(412.))), px(28.), cx));
            assert_eq!(view.scrolls.entries(), [(at(4), px(100.))]);
        })
        .unwrap();
    close(&mut cx);
}

#[gpui::test]
fn open_buffer_forgets_the_old_anchors(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);
    let old = anchors(window, &mut cx)[0];
    let first = buffer_of(window, &mut cx);
    let other = gpui::AppContext::new(&mut cx, |_| Buffer::new(block()));

    window
        .update(&mut cx, |view, _, cx| view.open_buffer(other, None, cx))
        .unwrap();
    cx.run_until_parked();

    assert!(!is_live(&first, old, &mut cx));
    assert!(anchors(window, &mut cx).is_empty());
    window
        .update(&mut cx, |view, _, _| assert_x(view.block_x(0), 0.))
        .unwrap();
}

#[gpui::test]
fn restore_reanchors(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);
    let old = anchors(window, &mut cx)[0];
    let view = ViewState {
        selection: Selection::at(at(0)),
        top: TopLine::default(),
        scrolls: vec![(at(0), px(60.))],
    };

    window
        .update(&mut cx, |editor, _, cx| editor.restore(view, cx))
        .unwrap();
    cx.run_until_parked();

    let buffer = buffer_of(window, &mut cx);
    assert!(!is_live(&buffer, old, &mut cx));
    let now = anchors(window, &mut cx);
    assert_eq!(now.len(), 1);
    assert!(is_live(&buffer, now[0], &mut cx));
    window
        .update(&mut cx, |view, _, _| assert_x(view.block_x(0), 60.))
        .unwrap();
    close(&mut cx);
}

fn scrolled_at(start: ByteOffset) -> ViewState {
    ViewState {
        selection: Selection::at(at(0)),
        top: TopLine::default(),
        scrolls: vec![(start, px(60.))],
    }
}

/// Unclipped, the anchor would panic past the end; clipped, the start is no block start and drops.
#[gpui::test]
fn restore_clips_a_start_past_the_end(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let past = at(block().len() + 10);

    window
        .update(&mut cx, |editor, _, cx| {
            editor.restore(scrolled_at(past), cx);
            editor.refresh(cx);
        })
        .unwrap();

    assert!(anchors(window, &mut cx).is_empty());
}

#[gpui::test]
fn park_clips_a_start_past_the_end(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    let buffer = buffer_of(window, &mut cx);
    let end = at(block().len());

    let unparked =
        cx.update(|_, cx| ParkedView::new(buffer, scrolled_at(at(end.get() + 10)), cx).unpark(cx));

    assert_eq!(unparked.scrolls, [(end, px(60.))]);
}

#[gpui::test]
fn peek_reads_the_parked_offsets_and_keeps_them(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);

    let (peeked, unparked) = window
        .update(&mut cx, |view, _, cx| {
            let parked = view.park(cx);
            view.buffer().update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.insert(at(0), "abc\n"));
            });
            let peeked = parked.peek(cx);
            (peeked, parked.unpark(cx))
        })
        .unwrap();

    assert_eq!(peeked.scrolls, [(at(4), px(100.))]);
    assert_eq!(unparked.scrolls, peeked.scrolls);
    close(&mut cx);
}

#[gpui::test]
fn a_parked_view_keeps_its_offsets_across_an_edit(cx: &mut TestAppContext) {
    let (window, mut cx) = swiped(cx, -100.);

    let unparked = window
        .update(&mut cx, |view, _, cx| {
            let parked = view.park(cx);
            view.buffer().update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.insert(at(0), "abc\n"));
            });
            parked.unpark(cx)
        })
        .unwrap();

    assert_eq!(unparked.scrolls, [(at(4), px(100.))]);
    close(&mut cx);
}

#[test]
fn a_saved_view_drops_scrolls() {
    let view = ViewState {
        selection: Selection::at(at(0)),
        top: TopLine::default(),
        scrolls: vec![(at(0), px(60.))],
    };

    let back = ViewState::from(SavedView::from(&view));

    assert!(back.scrolls.is_empty());
}
