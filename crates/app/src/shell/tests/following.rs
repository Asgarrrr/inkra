use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    Bounds, Focusable, TestAppContext, VisualTestContext, WindowBounds, WindowHandle, WindowOptions,
};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::bootstrap::{Launch, root};
use crate::editor::LinkFollowed;
use crate::keymap::key_bindings;
use crate::open::Open;
use crate::shell::AppShell;
use crate::test_support::entry_id;

use super::{activate, check, editor, open_vault, showing, tab_notes, vault_names};

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

/// Following a link to the note showing is the user's last word, as a click on it is.
#[gpui::test]
fn following_a_link_to_the_note_showing_during_a_slow_load_keeps_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[a]]");
    fs.insert_file("/vault/b.md", "beta");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, cx, b);
    cx.run_until_parked();
    let link = inkra_markdown::links("[[a]]").remove(0);
    window
        .update(cx, |shell, _, cx| shell.follow(&link, Open::Here, cx))
        .unwrap_or_else(|_| panic!("the window is open"));
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, cx), Some(a));
}

#[gpui::test]
fn cmd_shift_enter_on_a_wikilink_opens_its_note_in_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);
    let a = showing(&window, cx);

    visual.simulate_keystrokes("cmd-shift-enter");
    visual.run_until_parked();

    let (tabs, active) = tab_notes(&window, cx);
    assert_eq!((tabs.len(), tabs[0], active), (2, a, 1));
    assert_eq!(shown_text(&window, cx), "beta");
    check(&window, cx);
}

/// The link resolves from the note it is written in, not from the new tab.
#[gpui::test]
fn a_relative_link_in_a_new_tab_resolves_from_the_note_left(cx: &mut TestAppContext) {
    let (window, visual) = showing_a(
        cx,
        &[
            ("a.md", "[x](sub/c.md)"),
            ("c.md", "wrong"),
            ("sub/c.md", "gamma"),
        ],
    );
    let a = showing(&window, cx);
    let link = inkra_markdown::links("[x](sub/c.md)").remove(0);

    editor(&window, cx).update(cx, |_, cx| {
        cx.emit(LinkFollowed {
            link,
            open: Open::NewTab,
        });
    });
    visual.run_until_parked();

    let (tabs, active) = tab_notes(&window, cx);
    assert_eq!((tabs.len(), tabs[0], active), (2, a, 1));
    assert_eq!(shown_text(&window, cx), "gamma");
    check(&window, cx);
}

/// `/notes/a.md` launched by path beside a `b.md`, its editor focused and its caret at 0.
fn detached(cx: &mut TestAppContext, text: &str) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", text);
    fs.insert_file("/notes/b.md", "beta");
    let loaded = futures::executor::block_on(fs.load(Path::new("/notes/a.md"))).unwrap();
    let launch = Launch::Note(PathBuf::from("/notes/a.md"), loaded.text, loaded.stamp);
    let window = cx
        .update(|cx| {
            let bounds = Bounds::maximized(None, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| root(fs as Arc<dyn Fs>, launch, window, cx),
            )
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(&mut visual, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();
    visual.simulate_keystrokes("cmd-up");
    (window, visual)
}

#[gpui::test]
fn a_url_in_a_detached_note_opens_outside_the_app(cx: &mut TestAppContext) {
    let (window, mut visual) = detached(cx, "[x](https://x.org/p)");

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(visual.opened_url().as_deref(), Some("https://x.org/p"));
    assert_eq!(showing(&window, &mut visual), Some(EntryId::DETACHED));
}

#[gpui::test]
fn a_wikilink_in_a_detached_note_changes_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = detached(cx, "[[b]]");

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(visual.opened_url(), None);
    assert_eq!(showing(&window, &mut visual), Some(EntryId::DETACHED));
    assert_eq!(tab_notes(&window, &mut visual).0.len(), 1);
}
