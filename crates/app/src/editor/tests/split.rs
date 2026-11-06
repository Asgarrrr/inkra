//! Two editors on one buffer: the active one live, the inactive one pinned (splits spec, S2).

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Context, Entity, TestAppContext, VisualTestContext, Window, div, prelude::*};
use inkra_text::{Buffer, ByteOffset};

use super::at;
use crate::buffer_events::Reloaded;
use crate::editor::EditorView;
use crate::editor::actions::Undo;
use crate::text_input::InputHost;

/// Draws both editors, so a stale pinned selection reaches the layout.
struct Pair {
    a: Entity<EditorView>,
    b: Entity<EditorView>,
}

impl Render for Pair {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .child(div().flex_1().child(self.a.clone()))
            .child(div().flex_1().child(self.b.clone()))
    }
}

struct Split {
    a: Entity<EditorView>,
    b: Entity<EditorView>,
    buffer: Entity<Buffer>,
    cx: VisualTestContext,
}

fn split(cx: &mut TestAppContext, text: &str) -> Split {
    let buffer = cx.new(|_| Buffer::new(text));
    let shared = buffer.clone();
    let window = cx.add_window(move |_, cx| Pair {
        a: cx.new(|cx| EditorView::new(shared.clone(), cx)),
        b: cx.new(|cx| EditorView::new(shared, cx)),
    });
    let (a, b) = window
        .update(cx, |pair, _, _| (pair.a.clone(), pair.b.clone()))
        .unwrap();
    let cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    Split { a, b, buffer, cx }
}

fn select(view: &Entity<EditorView>, range: std::ops::Range<usize>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let buffer = view.input.buffer().clone();
        let buffer = buffer.read(cx);
        view.input
            .state_mut()
            .move_to(buffer, at(range.start), false);
        view.input.state_mut().move_to(buffer, at(range.end), true);
    });
}

fn type_in(view: &Entity<EditorView>, text: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.replace_text_in_range(None, text, window, cx);
    });
    cx.run_until_parked();
}

fn pin(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, EditorView::pin);
}

fn unpin(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, EditorView::unpin);
}

fn cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> ByteOffset {
    view.read_with(cx, |view, _| view.cursor())
}

#[gpui::test]
fn a_pinned_caret_follows_an_insert_before_it(cx: &mut TestAppContext) {
    let Split { a, b, mut cx, .. } = split(cx, "hello world");
    select(&b, 5..5, &mut cx);
    pin(&b, &mut cx);

    select(&a, 0..0, &mut cx);
    type_in(&a, "xy", &mut cx);

    assert_eq!(cursor(&b, &mut cx), at(7));
    unpin(&b, &mut cx);
}

#[gpui::test]
fn a_pinned_caret_survives_deleting_everything(cx: &mut TestAppContext) {
    let Split { a, b, mut cx, .. } = split(cx, "hello world");
    select(&b, 5..5, &mut cx);
    pin(&b, &mut cx);

    select(&a, 0..11, &mut cx);
    type_in(&a, "", &mut cx);

    assert_eq!(cursor(&b, &mut cx), at(0));
    assert!(
        b.read_with(&cx, |b, _| b.input.last_layout().is_some()),
        "the pinned editor was not drawn"
    );
    unpin(&b, &mut cx);
}

/// A refresh before the effects flush, as a reshape runs: `Reloaded` then finds the anchors already moved.
#[gpui::test]
fn a_reload_moves_a_pinned_caret_once(cx: &mut TestAppContext) {
    let Split {
        b, buffer, mut cx, ..
    } = split(cx, "hello world");
    select(&b, 5..5, &mut cx);
    pin(&b, &mut cx);

    cx.update(|_, cx| {
        buffer.update(cx, |buffer, cx| {
            let diff = Arc::new(inkra_text::diff(
                buffer.version(),
                "hello world",
                "abchello world",
            ));
            assert!(buffer.apply_diff(&diff));
            cx.emit(Reloaded(diff));
            cx.notify();
        });
        b.update(cx, |b, cx| b.refresh(cx));
    });
    cx.run_until_parked();

    assert_eq!(cursor(&b, &mut cx), at(8));
    unpin(&b, &mut cx);
}

#[gpui::test]
fn a_switch_seals_the_undo_group(cx: &mut TestAppContext) {
    let Split {
        a,
        b,
        buffer,
        mut cx,
    } = split(cx, "");
    pin(&b, &mut cx);

    type_in(&a, "a", &mut cx);
    pin(&a, &mut cx);
    unpin(&b, &mut cx);
    type_in(&b, "b", &mut cx);
    b.update_in(&mut cx, |b, window, cx| b.undo(&Undo, window, cx));

    assert_eq!(
        buffer.read_with(&cx, |buffer, _| buffer.text().to_owned()),
        "a"
    );
    unpin(&a, &mut cx);
}

#[gpui::test]
fn rebinding_a_pinned_editor_moves_its_pin(cx: &mut TestAppContext) {
    let Split { b, mut cx, .. } = split(cx, "hello world");
    let other = cx.new(|_| Buffer::new("other note"));
    pin(&b, &mut cx);

    b.update(&mut cx, |b, cx| b.open_buffer(other.clone(), None, cx));
    cx.run_until_parked();
    other.update(&mut cx, |other, cx| {
        other.transact_at(Instant::now(), |other| other.insert(at(0), "xy"));
        cx.notify();
    });
    cx.run_until_parked();

    assert!(b.read_with(&cx, |b, _| b.is_pinned()));
    assert_eq!(cursor(&b, &mut cx), at(2));
    unpin(&b, &mut cx);
}

#[gpui::test]
fn two_views_share_one_parse(cx: &mut TestAppContext) {
    let Split { a, b, mut cx, .. } = split(cx, "# hello");
    type_in(&a, " *world*", &mut cx);

    let markdown = |view: &Entity<EditorView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| Rc::clone(&view.markdown))
    };
    assert!(Rc::ptr_eq(&markdown(&a, &mut cx), &markdown(&b, &mut cx)));
}

#[gpui::test]
fn pin_and_unpin_are_idempotent(cx: &mut TestAppContext) {
    let Split { a, b, mut cx, .. } = split(cx, "hello world");
    select(&b, 5..5, &mut cx);

    pin(&b, &mut cx);
    pin(&b, &mut cx);
    assert!(b.read_with(&cx, |b, _| b.is_pinned()));
    select(&a, 0..0, &mut cx);
    type_in(&a, "xy", &mut cx);
    assert_eq!(cursor(&b, &mut cx), at(7));

    unpin(&b, &mut cx);
    unpin(&b, &mut cx);
    assert!(!b.read_with(&cx, |b, _| b.is_pinned()));
    assert_eq!(cursor(&b, &mut cx), at(7));
}
