//! A feature's published marks colour the editor's text (decoration layer spec, S1).

use std::time::Instant;

use gpui::{Entity, Hsla, TestAppContext, WindowHandle, prelude::*};
use inkra_text::Buffer;

use super::{at, window_on};
use crate::decorations::{self, MarkStyle};
use crate::editor::EditorView;
use crate::editor::layout::shape_count;
use crate::feature::fakes::{TODO_DECL, TODO_LAYER};
use crate::theme::{self, ColorToken};

/// Publishes the warning colour over `TODO` at the buffer's current edit mark.
fn publish_todo(buffer: &Entity<Buffer>, cx: &mut TestAppContext) {
    cx.update(|cx| {
        let warning = MarkStyle {
            color: Some(ColorToken::Warning),
        };
        let mark = buffer.read(cx).edit_mark();
        let marks = vec![(at(0)..at(4), warning)];
        decorations::publish(
            TODO_LAYER,
            &buffer.downgrade(),
            at(0)..at(10),
            marks,
            mark,
            cx,
        );
    });
    cx.run_until_parked();
}

/// The first line's runs: each one's length, and whether it is in the warning colour.
fn warning_runs(window: WindowHandle<EditorView>, cx: &mut TestAppContext) -> Vec<(usize, bool)> {
    window
        .update(cx, |view, _, cx| {
            let warning = Hsla::from(theme::current(cx).colors.warning);
            let runs = view.input.last_layout().unwrap().run_colors(0);
            runs.into_iter()
                .map(|(len, color)| (len, color == warning))
                .collect()
        })
        .unwrap()
}

#[gpui::test]
fn a_publish_colours_its_range(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    cx.run_until_parked();
    let before = shape_count();

    publish_todo(&buffer, cx);

    assert_eq!(warning_runs(window, cx), [(4, true), (5, false)]);
    assert_eq!(shape_count() - before, 1);
}

#[gpui::test]
fn an_edit_hides_the_marks_until_they_are_published_again(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    publish_todo(&buffer, cx);

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(at(9), "!"));
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(warning_runs(window, cx), [(10, false)]);

    // The layer is lost and dropped this publish; the cleared layer still reshapes once.
    let before = shape_count();
    publish_todo(&buffer, cx);
    assert_eq!(shape_count() - before, 1);
    assert_eq!(warning_runs(window, cx), [(10, false)]);

    publish_todo(&buffer, cx);
    assert_eq!(warning_runs(window, cx), [(4, true), (6, false)]);
}

#[gpui::test]
#[should_panic(expected = "undeclared layer")]
fn a_publish_to_an_undeclared_layer_panics(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO")));

    cx.update(|cx| {
        let mark = buffer.read(cx).edit_mark();
        decorations::publish(
            TODO_LAYER,
            &buffer.downgrade(),
            at(0)..at(4),
            Vec::new(),
            mark,
            cx,
        );
    });
}
