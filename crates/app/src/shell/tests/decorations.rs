//! The test feature colours a note from its listener, which hears each change once (decoration
//! layer spec, D4).

use std::time::Instant;

use gpui::{Focusable, Hsla, TestAppContext, VisualTestContext, WindowHandle};

use super::features::shell_with;
use super::{at, editor, open};
use crate::decorations;
use crate::feature::fakes::{self, CATALOG, TODO_CATALOG};
use crate::feature::{Manifest, Registry};
use crate::settings::Switches;
use crate::shell::AppShell;
use crate::{test_support, theme};

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

/// The first line's runs: each one's length, and whether it is in the warning colour.
fn warning_runs(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<(usize, bool)> {
    let editor = editor(&window, cx);
    cx.update(|_, cx| {
        let warning = Hsla::from(theme::current(cx).colors.warning);
        let runs = editor.read(cx).drawn_layout().run_colors(0);
        runs.into_iter()
            .map(|(len, color)| (len, color == warning))
            .collect()
    })
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
