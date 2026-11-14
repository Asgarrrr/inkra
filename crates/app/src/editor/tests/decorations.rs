//! A feature's published marks colour the editor's text and follow its edits (decoration layer
//! spec, S1, S2 and D8).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, Entity, Hsla, TestAppContext, WindowHandle, prelude::*};
use inkra_text::Buffer;

use super::{at, dispatching, window_on};
use crate::decorations::{self, LayerDecl, LayerId, MarkStyle};
use crate::editor::EditorView;
use crate::editor::layout::shape_count;
use crate::feature::FeatureId;
use crate::feature::fakes::{self, TODO_DECL, TODO_FAKE, TODO_LAYER};
use crate::theme::{self, ColorToken};

/// Publishes `token` over `mark` on `layer` at the buffer's current edit mark, in one chunk
/// covering the buffer.
fn publish_color(
    layer: LayerId,
    token: ColorToken,
    buffer: &Entity<Buffer>,
    mark: Range<usize>,
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let style = MarkStyle { color: Some(token) };
        let (at_mark, len) = (buffer.read(cx).edit_mark(), buffer.read(cx).len());
        let marks = vec![(at(mark.start)..at(mark.end), style)];
        decorations::publish(layer, &buffer.downgrade(), at(0)..len, marks, at_mark, cx);
    });
    cx.run_until_parked();
}

pub(super) fn publish_warning(
    buffer: &Entity<Buffer>,
    mark: Range<usize>,
    cx: &mut TestAppContext,
) {
    publish_color(TODO_LAYER, ColorToken::Warning, buffer, mark, cx);
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
    cx.update(fakes::declare_todo);
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
    cx.update(fakes::declare_todo);
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
    cx.update(fakes::declare_todo);
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
        cx.update(fakes::declare_todo);
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
    visual.update(|_, cx| fakes::declare_todo(cx));
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
fn the_later_layer_in_the_catalog_wins_whatever_the_publish_order(cx: &mut TestAppContext) {
    static LATER: LayerDecl = LayerDecl {
        id: LayerId::new(FeatureId::new("todo_fake"), "later"),
        open: None,
    };
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| {
        decorations::declare([&TODO_DECL, &LATER], cx);
        decorations::enable([TODO_FAKE.id], cx);
    });

    publish_color(LATER.id, ColorToken::Warning, &buffer, 0..4, cx);
    publish_color(TODO_LAYER, ColorToken::Accent, &buffer, 0..4, cx);

    assert_eq!(warning_runs(window, cx), [(4, true), (5, false)]);
}

#[gpui::test]
fn switching_off_a_layer_without_marks_keeps_the_layout(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| {
        fakes::declare_todo(cx);
        decorations::sync(&buffer, cx);
    });
    cx.run_until_parked();
    let before = shape_count();

    cx.update(|cx| decorations::enable([], cx));
    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();

    assert_eq!(shape_count(), before);
}

/// `other` published last, then left; the switch off must still give `buffer` a revision it never had.
#[gpui::test]
fn switching_off_reshapes_a_buffer_another_published_after(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO here\n")));
    let window = window_on(cx, buffer.clone());
    cx.update(fakes::declare_todo);
    publish_warning(&buffer, 0..4, cx);
    let other = cx.update(|cx| cx.new(|_| Buffer::new("TODO")));
    publish_warning(&other, 0..4, cx);
    drop(other);
    let tracked = cx.update(|cx| cx.new(|_| Buffer::new("")));
    cx.update(|cx| decorations::sync(&tracked, cx));

    cx.update(|cx| decorations::enable([], cx));
    cx.run_until_parked();

    assert_eq!(warning_runs(window, cx), [(9, false)]);
}

#[gpui::test]
fn a_released_buffer_is_untracked_once_another_is_tracked(cx: &mut TestAppContext) {
    cx.update(fakes::declare_todo);
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

#[gpui::test]
fn an_open_past_its_deadline_returns_early_and_the_rest_publishes_after_landing(
    cx: &mut TestAppContext,
) {
    let text = "TODO a TODO\n";
    let past = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let opened = decorations::run_openers(text, &[(TODO_LAYER, fakes::open_todo)], past);
    let (_, early) = &opened[0];
    assert!(early.chunks.iter().all(|(_, marks)| marks.is_empty()));
    let stop = early
        .seed
        .as_ref()
        .and_then(|seed| seed.downcast_ref::<usize>());
    assert_eq!(stop, Some(&0));

    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    let window = window_on(cx, buffer.clone());
    let _listening = cx.update(|cx| {
        fakes::declare_todo(cx);
        decorations::seed(&buffer, opened, cx);
        decorations::listen(TODO_LAYER, Rc::new(fakes::todo_listener), cx)
    });
    cx.run_until_parked();

    assert_eq!(fakes::heard(), ["opened+seed"]);
    assert_eq!(warning_runs(window, cx), [(4, true), (3, false), (4, true)]);
}

thread_local! {
    static DEADLINES: RefCell<Vec<Instant>> = const { RefCell::new(Vec::new()) };
}

fn record_deadline(_: &str, deadline: Instant) -> decorations::Opened {
    DEADLINES.with_borrow_mut(|deadlines| deadlines.push(deadline));
    decorations::Opened {
        chunks: Vec::new(),
        seed: None,
    }
}

#[test]
fn every_open_function_of_a_note_shares_one_deadline_8_ms_after_the_text() {
    let ready = Instant::now();
    let openers = [(TODO_LAYER, record_deadline as decorations::OpenFn); 2];
    decorations::run_openers("", &openers, ready);

    let deadline = ready + Duration::from_millis(8);
    assert_eq!(DEADLINES.take(), [deadline, deadline]);
}

#[gpui::test]
fn a_seed_waits_for_the_listener(cx: &mut TestAppContext) {
    cx.update(fakes::declare_todo);
    for (edited, heard) in [(false, "opened+seed"), (true, "opened")] {
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("TODO\n")));
        let _window = window_on(cx, buffer.clone());
        cx.update(|cx| {
            let opened = decorations::run_openers(
                "TODO\n",
                &[(TODO_LAYER, fakes::open_todo)],
                Instant::now(),
            );
            decorations::seed(&buffer, opened, cx);
        });
        cx.run_until_parked();
        if edited {
            buffer.update(cx, |buffer, cx| {
                buffer.transact_at(Instant::now(), |buffer| buffer.insert(at(0), "!"));
                cx.notify();
            });
        }

        let listening =
            cx.update(|cx| decorations::listen(TODO_LAYER, Rc::new(fakes::todo_listener), cx));
        cx.run_until_parked();

        assert_eq!(fakes::heard(), [heard], "edited: {edited}");
        drop(listening);
    }
}

/// The view drew the buffer plain first; only the seed's revision makes its next frame reshape.
#[gpui::test]
fn a_seed_after_a_frame_colours_the_next_one(cx: &mut TestAppContext) {
    let text = "TODO a\n";
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    let window = window_on(cx, buffer.clone());
    cx.update(fakes::declare_todo);
    cx.run_until_parked();

    cx.update(|cx| {
        let ready = Instant::now() + Duration::from_secs(3600);
        let opened = decorations::run_openers(text, &[(TODO_LAYER, fakes::open_todo)], ready);
        decorations::seed(&buffer, opened, cx);
    });
    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();

    assert_eq!(warning_runs(window, cx), [(4, true), (2, false)]);
}

/// The opener was read while the layer was on; the switch off before the landing drops its marks.
#[gpui::test]
fn a_seed_for_a_layer_switched_off_since_its_open_draws_nothing(cx: &mut TestAppContext) {
    let text = "TODO a\n";
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    let window = window_on(cx, buffer.clone());
    cx.update(|cx| {
        fakes::declare_todo(cx);
        let ready = Instant::now() + Duration::from_secs(3600);
        let opened = decorations::run_openers(text, &decorations::openers(cx), ready);
        decorations::enable([], cx);
        decorations::seed(&buffer, opened, cx);
    });
    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();

    assert_eq!(warning_runs(window, cx), [(6, false)]);
}
