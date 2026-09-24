use std::sync::Arc;

use gpui::{Modifiers, TestAppContext, VisualTestContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::keymap::key_bindings;
use crate::test_support::entry_id;

use super::{activate, editor, open_vault, showing};

#[gpui::test]
fn clicking_a_backlink_opens_its_source(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha links [[b]]");
    fs.insert_file("/vault/b.md", "beta");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let b = entry_id(&notes, cx, "b");
    activate(&sidebar, cx, b);
    cx.run_until_parked();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let row = visual
        .debug_bounds("backlink-0")
        .expect("the panel shows a's backlink");
    visual.simulate_click(row.center(), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(showing(&window, cx), Some(entry_id(&notes, cx, "a")));
    editor(&window, cx).read_with(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "alpha links [[b]]");
    });
}

#[gpui::test]
fn the_panel_follows_the_note_showing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "[[c]]");
    fs.insert_file("/vault/c.md", "");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let sources = |window: &gpui::WindowHandle<crate::shell::AppShell>, cx: &mut TestAppContext| {
        window
            .update(cx, |shell, _, cx| shell.backlinks.read(cx).source_names())
            .expect("the shell is open")
    };

    let b = entry_id(&notes, cx, "b");
    activate(&sidebar, cx, b);
    cx.run_until_parked();
    assert_eq!(sources(&window, cx), ["a"]);

    let c = entry_id(&notes, cx, "c");
    activate(&sidebar, cx, c);
    cx.run_until_parked();
    assert_eq!(sources(&window, cx), ["b"]);

    let trashed = notes.update(cx, |notes, cx| notes.trash(c, cx));
    cx.run_until_parked();
    drop(trashed);
    assert_eq!(showing(&window, cx), None);
    assert_eq!(sources(&window, cx), Vec::<String>::new());
}

/// A note that links to itself is its own backlink; clicking that row must not reopen it.
#[gpui::test]
fn clicking_the_note_showing_in_its_own_backlinks_keeps_its_caret(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "see [[a]] here");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-up right right");
    visual.run_until_parked();
    assert_eq!(
        editor(&window, cx)
            .read_with(cx, |editor, _| editor.cursor())
            .0,
        2
    );
    let row = visual.debug_bounds("backlink-0").expect("a lists itself");
    visual.simulate_click(row.center(), Modifiers::none());
    visual.run_until_parked();

    let caret = editor(&window, cx).read_with(cx, |editor, _| editor.cursor());
    assert_eq!(caret.0, 2);
}
