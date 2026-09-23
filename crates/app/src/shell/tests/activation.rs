use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    Bounds, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, point, prelude::*, px,
};
use inkra_text::Buffer;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Vault};

use crate::bootstrap::{Launch, root};
use crate::keymap::key_bindings;
use crate::shell::{AppShell, EditorView, NoteStore, Save};
use crate::sidebar::row_center;
use crate::test_support::entry_id;

use super::{activate, editor, open_vault, two_notes};

#[gpui::test]
fn a_keystroke_through_app_shell_reaches_the_editor(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
    let notes = cx.update(|cx| cx.new(|cx| NoteStore::new(Arc::new(MemFs::new()), None, cx)));
    let window = cx.add_window(move |_, cx| AppShell::new(Some(editor), notes, cx));
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_input("hi");

    window
        .update(&mut visual, |_, _, cx| {
            assert_eq!(buffer.read(cx).text(), "hi");
        })
        .unwrap();
}

#[gpui::test]
fn clicking_a_row_hands_its_buffer_to_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");

    let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();

    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "alpha");
    });
}

/// A `current` the editor disagrees with saves nothing: `⌘S` returns early on the clean note.
#[gpui::test]
fn an_abandoned_load_does_not_supersede_a_later_activation(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, cx, b);
    cx.run_until_parked();
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    gate.release();
    cx.run_until_parked();

    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(
            editor.buffer().read(cx).text(),
            "alpha",
            "the last note activated is the note the editor ends on"
        );
    });
    notes.update(cx, |notes, _| {
        assert_eq!(
            notes.current(),
            Some(a),
            "`save_current` reads this; pointing it at the abandoned note \
             makes `cmd-s` a silent no-op"
        );
    });
}

#[gpui::test]
fn the_current_note_is_the_one_in_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

    let gate_a = fs.block_load("/vault/a.md");
    let gate_b = fs.block_load("/vault/b.md");

    activate(&sidebar, cx, a);
    cx.run_until_parked();
    activate(&sidebar, cx, b);
    cx.run_until_parked();

    gate_b.release();
    cx.run_until_parked();
    gate_a.release();
    cx.run_until_parked();

    let current = notes
        .update(cx, |notes, _| notes.current())
        .expect("a note is current");
    assert_eq!(current, b, "the last activation wins");

    let in_store = notes.update(cx, |notes, _| {
        notes.buffer_for(current).map(Entity::entity_id)
    });
    let in_editor = editor(&window, cx).update(cx, |editor, _| editor.buffer().entity_id());
    assert_eq!(
        in_store,
        Some(in_editor),
        "the current note and the editor's buffer are the same entity"
    );
}

/// The rebind is one turn late; `⌘S` in that turn silently writes nothing.
#[gpui::test]
fn an_activation_records_the_note_only_once_the_editor_holds_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

    activate(&sidebar, cx, a);
    cx.run_until_parked();
    activate(&sidebar, cx, b);
    cx.run_until_parked();

    // Not parked: this is the turn `⌘S` can land in.
    activate(&sidebar, cx, a);

    assert_eq!(
        notes.update(cx, |notes, _| notes.current()),
        Some(b),
        "the editor still holds `b`, so `b` is still the note a save writes"
    );
    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "beta");
    });

    cx.run_until_parked();
    assert_eq!(notes.update(cx, |notes, _| notes.current()), Some(a));
    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "alpha");
    });
}

/// An editor built but never focused swallows every keystroke via `track_focus`.
#[gpui::test]
fn typing_reaches_a_note_opened_after_launch(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");

    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_input("hi");
    visual.run_until_parked();

    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(
            editor.buffer().read(cx).text(),
            "hi",
            "the editor created by the activation never received focus"
        );
    });
}

/// Asserts reachability, not an effect: `save_current`'s `None` guard makes
/// "no panic and no write" true whether or not it was reached at all.
#[gpui::test]
fn the_save_action_reaches_the_shell_with_no_note_open(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();
    cx.run_until_parked();

    let reachable = window
        .update(cx, |_, window, cx| window.is_action_available(&Save, cx))
        .unwrap();
    assert!(
        reachable,
        "`cmd-s` cannot reach `AppShell::save` from the empty state"
    );

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-s");
    visual.run_until_parked();
}

#[gpui::test]
fn activating_a_note_replaces_the_empty_state(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    assert!(
        window
            .update(cx, |shell, _, _| shell.editor.is_none())
            .unwrap()
    );

    activate(&sidebar, cx, a);
    cx.run_until_parked();

    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "alpha");
    });
}

/// The only test using `root`, so the only one that would catch a regression to an unconditional editor.
#[gpui::test]
fn root_builds_an_editor_only_for_a_named_file(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let scanned =
        futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault"))).expect("scan");
    let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md"))).expect("load");

    for (launched, expected) in [
        (Launch::Nothing, false),
        (Launch::Vault(scanned), false),
        (
            Launch::Note(
                PathBuf::from("/vault/a.md"),
                loaded.text.clone(),
                loaded.stamp,
            ),
            true,
        ),
    ] {
        let fs = Arc::clone(&fs) as Arc<dyn Fs>;
        // `open_window`, not `add_window`: `root` hands back an already-built `Entity<AppShell>`.
        let window = cx
            .update(|cx| {
                let bounds = Bounds::maximized(None, cx);
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |_, cx| root(fs, launched, cx),
                )
            })
            .expect("open the window");
        let has_editor = window
            .update(cx, |shell, _, _| shell.editor.is_some())
            .unwrap();
        assert_eq!(has_editor, expected);
    }
}
