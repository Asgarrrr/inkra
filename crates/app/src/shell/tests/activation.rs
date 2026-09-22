//! Which note the editor ends up showing, and that a keystroke reaches it.

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

/// The one regression this file exists to guard: the existing
/// `simulate_keystrokes` tests in `editor/tests/` open a window with
/// `EditorView` as the direct root, so they never exercise `AppShell` at
/// all. Only this test opens the window the way `main.rs` actually does
/// — through `AppShell` — and proves a keystroke still reaches the
/// editor once it is no longer the window root.
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

/// The one end-to-end test of the seam: a real click, through the
/// subscription, into the editor. Nothing in `sidebar.rs` catches a
/// subscription wired to the wrong entity, or left unbound.
#[gpui::test]
fn clicking_a_row_hands_its_buffer_to_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");

    let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    // Vertical middle of the first row, past its `pl`.
    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();

    editor(&window, cx).update(cx, |editor, cx| {
        assert_eq!(editor.buffer().read(cx).text(), "alpha");
    });
}

/// A `current` the editor disagrees with saves *nothing*: the note it picks
/// is clean, so `⌘S` returns early and the dirty note stays dirty, silently.
/// Deterministic under the randomizing test scheduler
/// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:641-643`)
/// because only one load is in flight and the gate decides when it lands.
#[gpui::test]
fn an_abandoned_load_does_not_supersede_a_later_activation(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

    // Opened up front so the activation below takes the already-open path.
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

/// The gates release in the opposite order to the activations: against the
/// unfixed code both loads land and the last wins. Against the fixed code
/// activating `b` tears `a`'s whole chain down, so the order stops mattering.
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

/// Even with no load to cancel, the rebind is one turn late; `⌘S` in that
/// turn silently writes nothing. Hence `set_current` after the bind.
///
/// The emit hop costs no turn: the subscriber runs when the outermost
/// update flushes, so `activate` returns with the task spawned but unpolled.
#[gpui::test]
fn an_activation_records_the_note_only_once_the_editor_holds_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

    // Both opened, so the third activation takes the already-open path.
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    activate(&sidebar, cx, b);
    cx.run_until_parked();

    // Deliberately not parked: this is the turn `⌘S` can land in.
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

/// The whole reason this change is not four lines of `div`. The editor does
/// not exist when the window opens, so something has to move focus to it
/// when it appears. Nothing else can: `EditorView` takes keystrokes through
/// `track_focus`, called from its own `Render::render`, so an editor that is
/// built and rendered but never focused swallows every one of them.
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

/// Guards `AppShell`'s `track_focus` — see `shell.rs`'s `render` for why
/// its absence would swallow `cmd-s` with no note open.
///
/// Asserts reachability, not an effect, deliberately: `save_current`
/// returns at its `None` guard whether or not it was called, so "no panic
/// and no write" is true either way. `is_action_available`
/// (`window.rs:3187-3193`) asks the dispatch tree directly, reading the
/// *rendered* frame — which exists here because `open_window` draws once
/// before returning (`app.rs:1363-1368`).
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

    // And it does not panic on the way through.
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-s");
    visual.run_until_parked();
}

/// The transition, in one test: the empty state is what an activation
/// replaces, and it replaces it with the note that was activated.
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

/// The hole this change closes — see CLAUDE.md on `AppShell.editor` — and
/// the only test that guards it: every other test here builds `AppShell` by
/// hand with its own `None`, so only this test, which calls `root`, would
/// notice a regression to an unconditional editor.
///
/// It asserts the absence of an editor rather than the presence of the
/// empty state: `check.sh` renders nothing, so "the text was drawn" is not
/// a claim a `#[gpui::test]` can make. The `empty-no-vault` shot is that
/// check.
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
        // `App::open_window`, not `cx.add_window`: the test helpers all
        // build the root view themselves, and `root` hands back an already
        // built `Entity<AppShell>` — which is the shape `open_window`'s
        // builder takes and theirs do not.
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
