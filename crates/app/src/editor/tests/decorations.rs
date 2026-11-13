//! A feature's published marks colour the editor's text and follow its edits (decoration layer
//! spec, S1 and S2).

use std::ops::Range;
use std::time::Instant;

use gpui::{App, Entity, Hsla, TestAppContext, WindowHandle, prelude::*};
use inkra_text::Buffer;

use super::{at, dispatching, window_on};
use crate::decorations::{self, MarkStyle};
use crate::editor::EditorView;
use crate::editor::layout::shape_count;
use crate::feature::fakes::{TODO_DECL, TODO_LAYER};
use crate::theme::{self, ColorToken};

/// Publishes the warning colour over `mark` at the buffer's current edit mark, in one chunk
/// covering the buffer.
fn publish_warning(buffer: &Entity<Buffer>, mark: Range<usize>, cx: &mut TestAppContext) {
    cx.update(|cx| {
        let warning = MarkStyle {
            color: Some(ColorToken::Warning),
        };
        let (at_mark, len) = (buffer.read(cx).edit_mark(), buffer.read(cx).len());
        let marks = vec![(at(mark.start)..at(mark.end), warning)];
        decorations::publish(
            TODO_LAYER,
            &buffer.downgrade(),
            at(0)..len,
            marks,
            at_mark,
            cx,
        );
    });
    cx.run_until_parked();
}

impl EditorView {
    /// The first drawn line's runs: each one's length, and whether it is in the warning colour.
    pub(crate) fn warning_runs(&self, cx: &App) -> Vec<(usize, bool)> {
        let warning = Hsla::from(theme::current(cx).colors.warning);
        let runs = self.drawn_layout().run_colors(0);
        runs.into_iter()
            .map(|(len, color)| (len, color == warning))
            .collect()
    }
}

fn warning_runs(window: WindowHandle<EditorView>, cx: &mut TestAppContext) -> Vec<(usize, bool)> {
    window
        .update(cx, |view, _, cx| view.warning_runs(cx))
        .unwrap()
}

#[gpui::test]
fn a_publish_colours_its_range(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    cx.run_until_parked();
    let before = shape_count();

    publish_warning(&buffer, 0..4, cx);

    assert_eq!(warning_runs(window, cx), [(4, true), (5, false)]);
    assert_eq!(shape_count() - before, 1);
}

#[gpui::test]
fn a_frame_without_an_edit_reuses_the_coloured_layout(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    publish_warning(&buffer, 0..4, cx);
    let before = shape_count();

    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();

    assert_eq!(shape_count(), before);
    assert_eq!(warning_runs(window, cx), [(4, true), (5, false)]);
}

#[gpui::test]
fn an_edit_shifts_the_marks_and_an_overflow_hides_them_until_published_again(
    cx: &mut TestAppContext,
) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    publish_warning(&buffer, 0..4, cx);

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(at(0), "!"));
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        warning_runs(window, cx),
        [(1, false), (4, true), (5, false)]
    );

    // 258 splices: more than the 256 the log keeps, so the layer is lost.
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            for _ in 0..129 {
                buffer.insert(at(10), "?");
                buffer.delete(at(10)..at(11));
            }
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(warning_runs(window, cx), [(10, false)]);

    publish_warning(&buffer, 1..5, cx);
    assert_eq!(
        warning_runs(window, cx),
        [(1, false), (4, true), (5, false)]
    );
}

/// Undo brings back the version last shaped; the reinserted bytes are new, so nothing is coloured.
#[gpui::test]
fn a_delete_undone_out_of_sight_draws_no_stale_colour(cx: &mut TestAppContext) {
    // 0 pairs: the layer follows the two splices; 129 pairs: 260 splices, so it is lost.
    for redo_pairs in [0, 129] {
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
        let window = window_on(cx, buffer.clone());
        cx.update(|cx| decorations::declare([&TODO_DECL], cx));
        publish_warning(&buffer, 0..4, cx);

        buffer.update(cx, |buffer, cx| {
            buffer.transact_at(Instant::now(), |buffer| buffer.delete(at(0)..at(4)));
            buffer.undo();
            for _ in 0..redo_pairs {
                buffer.redo();
                buffer.undo();
            }
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(warning_runs(window, cx), [(9, false)], "{redo_pairs} pairs");
    }
}

#[gpui::test]
fn a_keystroke_shifts_the_colours_and_leaves_typed_text_plain(cx: &mut TestAppContext) {
    let (window, mut visual) = dispatching(cx, "a TODO b\n");
    let buffer = window
        .update(&mut visual, |view, _, _| view.input.buffer().clone())
        .unwrap();
    visual.update(|_, cx| decorations::declare([&TODO_DECL], cx));
    publish_warning(&buffer, 2..6, &mut visual);

    visual.simulate_keystrokes("cmd-up right right");
    visual.simulate_input("x");
    assert_eq!(
        warning_runs(window, &mut visual),
        [(3, false), (4, true), (2, false)]
    );

    visual.simulate_keystrokes("right right");
    visual.simulate_input("y");
    assert_eq!(
        warning_runs(window, &mut visual),
        [(3, false), (2, true), (1, false), (2, true), (2, false)]
    );
}

#[gpui::test]
fn a_released_buffer_is_untracked_once_another_is_tracked(cx: &mut TestAppContext) {
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    let released = cx.update(|cx| cx.new(|_| Buffer::new("TODO")));
    publish_warning(&released, 0..4, cx);
    drop(released);
    cx.run_until_parked();

    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO")));
    publish_warning(&buffer, 0..4, cx);

    assert_eq!(cx.update(|cx| decorations::tracked(cx)), 1);
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
