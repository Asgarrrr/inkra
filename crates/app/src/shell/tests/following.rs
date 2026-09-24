use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::keymap::key_bindings;
use crate::shell::AppShell;
use crate::test_support::entry_id;

use super::{activate, editor, open_vault, showing, vault_names};

/// A vault whose `a` is showing, its caret at 0 (`open_buffer` leaves it at the end), with the
/// index built.
fn showing_a(
    cx: &mut TestAppContext,
    files: &[(&str, &str)],
) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    for (name, text) in files {
        fs.insert_file(format!("/vault/{name}"), *text);
    }
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    assert_eq!(showing(&window, cx), Some(a));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-up");
    (window, visual)
}

fn shown_text(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> String {
    editor(window, cx).read_with(cx, |editor, cx| editor.buffer().read(cx).text().to_owned())
}

#[gpui::test]
fn cmd_enter_on_a_wikilink_opens_its_note(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(shown_text(&window, cx), "beta");
}

#[gpui::test]
fn a_relative_link_resolves_from_the_showing_note(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(
        cx,
        &[
            ("a.md", "[x](sub/c.md)"),
            ("c.md", "wrong"),
            ("sub/c.md", "gamma"),
        ],
    );

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(shown_text(&window, cx), "gamma");
}

#[gpui::test]
fn a_url_opens_outside_the_app(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[x](https://x.org/p)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url().as_deref(), Some("https://x.org/p"));
    assert_eq!(shown_text(&window, cx), "[x](https://x.org/p)");
}

#[gpui::test]
fn an_unresolved_link_changes_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[missing]]")]);
    let before = showing(&window, cx);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(showing(&window, cx), before);
    assert_eq!(cx.opened_url(), None);
    let notes = window
        .update(cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(vault_names(&notes, cx), ["a.md"]);
}

/// Rebinding the note showing would put its caret back at 0.
#[gpui::test]
fn a_link_to_the_note_showing_keeps_its_caret(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "x [[a]]")]);
    visual.simulate_keystrokes("end");
    let before = editor(&window, cx).read_with(cx, |editor, _| editor.cursor());

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(
        editor(&window, cx).read_with(cx, |editor, _| editor.cursor()),
        before
    );
}
