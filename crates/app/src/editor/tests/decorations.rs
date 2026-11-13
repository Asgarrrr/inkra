//! A feature's published marks colour the editor's text (decoration layer spec, S1).

use gpui::{Hsla, TestAppContext, prelude::*};
use inkra_text::Buffer;

use super::{at, window_on};
use crate::decorations::{self, MarkStyle};
use crate::editor::layout::shape_count;
use crate::feature::fakes::{TODO_DECL, TODO_LAYER};
use crate::theme::{self, ColorToken};

#[gpui::test]
fn a_publish_colours_its_range(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    cx.run_until_parked();
    let before = shape_count();

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

    window
        .update(cx, |view, _, cx| {
            let warning = Hsla::from(theme::current(cx).colors.warning);
            let runs = view.input.last_layout().unwrap().run_colors(0);
            assert_eq!(runs[0], (4, warning));
            assert_ne!(runs[1].1, warning);
        })
        .unwrap();
    assert_eq!(shape_count() - before, 1);
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
