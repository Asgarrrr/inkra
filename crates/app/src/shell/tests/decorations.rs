//! The test feature colours a note from its listener, which hears each change once (decoration
//! layer spec, D4).

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{AppContext, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_text::Buffer;

use super::features::shell_with;
use super::{at, editor, open};
use crate::decorations;
use crate::feature::fakes::{self, CATALOG, TODO_CATALOG, TODO_DECL, TODO_LAYER};
use crate::feature::{Manifest, Registry};
use crate::settings::Switches;
use crate::shell::AppShell;
use crate::test_support;

/// `/vault/a.md` holding `text`, open and focused behind a launched shell with `catalog`.
fn opened(
    cx: &mut TestAppContext,
    catalog: &'static [&'static Manifest],
    text: &str,
) -> (WindowHandle<AppShell>, VisualTestContext) {
    let (fs, window, mut visual) = shell_with(cx, Registry::new(catalog, &Switches::default()));
    fs.insert_file("/vault/a.md", text);
    visual.run_until_parked();
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    let a = test_support::entry_id(&notes, &mut visual, "a");
    open(&window, &mut visual, a);
    let editor = editor(&window, &mut visual);
    window
        .update(&mut visual, |_, window, cx| {
            window.focus(&editor.focus_handle(cx), cx);
        })
        .unwrap();
    visual.run_until_parked();
    (window, visual)
}

const DELAY: Duration = Duration::from_millis(50);

fn warning_runs(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<(usize, bool)> {
    let editor = editor(&window, cx);
    cx.update(|_, cx| editor.read(cx).warning_runs(cx))
}

#[gpui::test]
fn the_test_feature_colours_its_words_once_launched(cx: &mut TestAppContext) {
    let (window, mut visual) = opened(cx, TODO_CATALOG, "a TODO b TODO\n");

    assert_eq!(
        warning_runs(window, &mut visual),
        [(2, false), (4, true), (3, false), (4, true)]
    );
}

#[gpui::test]
fn each_change_is_heard_once(cx: &mut TestAppContext) {
    let (_window, mut visual) = opened(cx, TODO_CATALOG, "a TODO b TODO\n");
    assert_eq!(fakes::heard(), ["opened"]);

    visual.simulate_input("x");
    visual.run_until_parked();
    assert_eq!(fakes::heard(), ["edited"]);
}

#[gpui::test]
fn a_change_outside_the_gates_is_heard(cx: &mut TestAppContext) {
    let (window, mut visual) = opened(cx, TODO_CATALOG, "a TODO b TODO\n");
    fakes::heard();

    let buffer =
        editor(&window, &mut visual).read_with(&visual, |editor, _| editor.buffer().clone());
    buffer.update(&mut visual, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(at(0), "TODO "));
        cx.notify();
    });
    visual.run_until_parked();

    assert_eq!(fakes::heard(), ["edited"]);
    assert_eq!(
        warning_runs(window, &mut visual),
        [(4, true), (3, false), (4, true), (3, false), (4, true)]
    );
}

#[gpui::test]
fn a_feature_without_layers_tracks_no_buffer(cx: &mut TestAppContext) {
    let (_window, mut visual) = opened(cx, CATALOG, "a TODO b TODO\n");

    visual.simulate_input("x");
    visual.run_until_parked();
    assert_eq!(visual.update(|_, cx| decorations::tracked(cx)), 0);
}

/// The publishes computed before and after `a` land after `b` is typed; the one after `b` has not.
#[gpui::test]
fn a_late_publish_lands_mapped(cx: &mut TestAppContext) {
    fakes::set_delay(Some(DELAY));
    let (window, mut visual) = opened(cx, TODO_CATALOG, "TODO\n");
    visual.simulate_keystrokes("cmd-up");

    visual.simulate_input("a");
    visual.executor().advance_clock(DELAY / 2);
    visual.simulate_input("b");
    visual.executor().advance_clock(DELAY / 2);
    visual.run_until_parked();

    assert_eq!(warning_runs(window, &mut visual), [(2, false), (4, true)]);
}

/// The publish computed at open lands past the log, after the listener republished on `Lost`.
#[gpui::test]
fn an_overflow_sends_lost_and_a_full_publish_restores_the_colours(cx: &mut TestAppContext) {
    fakes::set_delay(Some(DELAY));
    let (window, mut visual) = opened(cx, TODO_CATALOG, "a TODO b TODO\n");
    fakes::set_delay(None);

    let buffer =
        editor(&window, &mut visual).read_with(&visual, |editor, _| editor.buffer().clone());
    buffer.update(&mut visual, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            for _ in 0..300 {
                buffer.insert(at(14), "x");
            }
        });
        cx.notify();
    });
    visual.executor().advance_clock(DELAY);
    visual.run_until_parked();

    assert_eq!(fakes::heard().last(), Some(&"lost"));
    assert_eq!(
        warning_runs(window, &mut visual),
        [(2, false), (4, true), (3, false), (4, true)]
    );
}

#[gpui::test]
fn a_publish_for_a_released_buffer_is_a_no_op(cx: &mut TestAppContext) {
    fakes::set_delay(Some(DELAY));
    cx.update(|cx| decorations::declare([&TODO_DECL], cx));
    let _listening =
        cx.update(|cx| decorations::listen(TODO_LAYER, Rc::new(fakes::todo_listener), cx));
    let released = cx.new(|_| Buffer::new("TODO\n"));
    cx.update(|cx| decorations::sync(&released, cx));
    drop(released);
    cx.run_until_parked();

    let buffer = cx.new(|_| Buffer::new("TODO\n"));
    cx.update(|cx| decorations::sync(&buffer, cx));
    cx.executor().advance_clock(DELAY);
    cx.run_until_parked();

    assert_eq!(cx.update(|cx| decorations::tracked(cx)), 1);
}
