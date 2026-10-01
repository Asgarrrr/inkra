use std::sync::Arc;

use gpui::{Focusable, Modifiers, TestAppContext, VisualTestContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::keymap::key_bindings;
use crate::test_support::entry_id;

use super::{activate, editor, open_vault, showing, tree_focused};

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
            .update(cx, |shell, _, cx| {
                shell.panes().backlinks.read(cx).source_names()
            })
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

/// The click is the user's last word: the slower activation before it must not land.
#[gpui::test]
fn clicking_the_note_showing_during_a_slow_load_keeps_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "see [[a]] here");
    fs.insert_file("/vault/b.md", "beta");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, &mut visual, b);
    visual.run_until_parked();
    let row = visual.debug_bounds("backlink-0").expect("a lists itself");
    visual.simulate_click(row.center(), Modifiers::none());
    gate.release();
    visual.run_until_parked();

    assert_eq!(showing(&window, cx), Some(a));
}

/// A click on the panel moves focus to the shell's root; opening another note moves it on to the
/// editor, so the note showing must too.
#[gpui::test]
fn from_the_tree_clicking_the_note_showing_in_its_own_backlinks_focuses_the_editor(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "see [[a]] here");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);

    let row = visual.debug_bounds("backlink-0").expect("a lists itself");
    visual.simulate_click(row.center(), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(showing(&window, cx), Some(a));
    let editor_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(editor_focused);
}
