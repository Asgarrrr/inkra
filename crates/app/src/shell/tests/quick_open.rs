use std::path::Path;
use std::sync::Arc;

use gpui::{Focusable, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use super::{activate, editor, open_vault, showing, tree_focused};
use crate::keymap::key_bindings;
use crate::shell::{AppShell, NoteStore, RECENT, remember};
use crate::test_support::entry_id;

/// Three notes shown in turn, so the MRU is c, b, a and c is showing.
fn three_shown(
    cx: &mut TestAppContext,
) -> (WindowHandle<AppShell>, VisualTestContext, [EntryId; 3]) {
    let fs = Arc::new(MemFs::new());
    for name in ["a", "b", "c"] {
        fs.insert_file(format!("/vault/{name}.md"), format!("{name} text"));
    }
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    let ids = ["a", "b", "c"].map(|name| entry_id(&notes, cx, name));
    for id in ids {
        activate(&sidebar, cx, id);
        cx.run_until_parked();
    }
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    (window, visual, ids)
}

fn has_modal(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.modal.is_some())
        .expect("the shell is open")
}

#[gpui::test]
fn cmd_p_opens_quick_open_and_cmd_p_again_closes_it(cx: &mut TestAppContext) {
    let (window, mut visual, _) = three_shown(cx);

    visual.simulate_keystrokes("cmd-p");
    assert!(has_modal(window, &mut visual));

    visual.simulate_keystrokes("cmd-p");
    assert!(!has_modal(window, &mut visual));
    let editor_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(editor_focused);
}

#[gpui::test]
fn cmd_p_does_nothing_without_a_vault(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = cx.update(|cx| {
        gpui::AppContext::new(cx, |cx| NoteStore::new(Arc::new(MemFs::new()), None, cx))
    });
    let window = cx.add_window(move |_, cx| AppShell::new(None, notes, cx));
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("the shell is open");
    let mut visual = VisualTestContext::from_window(window.into(), cx);

    visual.simulate_keystrokes("cmd-p");

    assert!(!has_modal(window, &mut visual));
}

#[gpui::test]
fn enter_opens_the_first_recent_note(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b, _c]) = three_shown(cx);

    // Apart, from a settled list; `cmd_p_then_enter_at_once_opens_the_previous_note` covers one batch.
    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("enter");

    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(!has_modal(window, &mut visual));
}

#[gpui::test]
fn down_then_enter_opens_the_second(cx: &mut TestAppContext) {
    let (window, mut visual, [a, _b, _c]) = three_shown(cx);

    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("down enter");

    assert_eq!(showing(&window, &mut visual), Some(a));
}

#[gpui::test]
fn up_moves_the_selection_back(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b, _c]) = three_shown(cx);

    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("down up enter");

    assert_eq!(showing(&window, &mut visual), Some(b));
}

#[gpui::test]
fn escape_closes_quick_open_and_the_editor_keeps_its_caret(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, _b, c]) = three_shown(cx);
    let before = editor(&window, &mut visual).update(&mut visual, |editor, _| editor.cursor());

    visual.simulate_keystrokes("cmd-p escape");

    assert!(!has_modal(window, &mut visual));
    assert_eq!(showing(&window, &mut visual), Some(c));
    assert_eq!(
        editor(&window, &mut visual).update(&mut visual, |editor, _| editor.cursor()),
        before
    );
}

#[gpui::test]
fn from_the_tree_cmd_p_then_escape_leaves_the_tree_focused(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-p escape");

    let tree_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(tree_focused);
}

#[gpui::test]
fn cmd_p_during_a_rename_keeps_the_rename_open(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    // Rename acts on the tree's selection, and nothing is selected yet.
    visual.simulate_keystrokes("down enter");
    assert!(sidebar.read_with(&visual, |sidebar, _| sidebar.is_editing()));

    visual.simulate_keystrokes("cmd-p");
    assert!(has_modal(window, &mut visual));
    assert!(sidebar.read_with(&visual, |sidebar, _| sidebar.is_editing()));

    visual.simulate_keystrokes("escape");
    assert!(
        sidebar.read_with(&visual, |sidebar, _| sidebar.is_editing()),
        "the escape closed the picker only"
    );
    let field_focused = sidebar.update_in(&mut visual, |sidebar, window, cx| {
        sidebar.field_is_focused(window, cx)
    });
    assert!(field_focused, "focus went back to the rename field");
}

#[gpui::test]
fn confirming_the_note_showing_keeps_its_caret(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, _b, c]) = three_shown(cx);
    visual.simulate_keystrokes("right right");
    let before = editor(&window, &mut visual).update(&mut visual, |editor, _| editor.cursor());

    // The note showing is ranked down, never hidden, once a query is typed.
    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("c");
    visual.simulate_keystrokes("enter");

    assert_eq!(showing(&window, &mut visual), Some(c));
    assert_eq!(
        editor(&window, &mut visual).update(&mut visual, |editor, _| editor.cursor()),
        before
    );
}

/// Picking another note focuses its editor, so picking the one showing must too.
#[gpui::test]
fn from_the_tree_picking_the_note_showing_focuses_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("a");
    visual.simulate_keystrokes("enter");

    assert_eq!(showing(&window, &mut visual), Some(a));
    assert!(!has_modal(window, &mut visual));
    let editor_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(editor_focused);
}

#[gpui::test]
fn a_click_on_a_row_opens_it_and_leaves_the_editor_selection_alone(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b, _c]) = three_shown(cx);
    visual.simulate_keystrokes("cmd-p");
    let row = visual.debug_bounds("picker-row-0").expect("row 0 is drawn");

    visual.simulate_click(row.center(), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(!has_modal(window, &mut visual));
}

#[test]
fn the_recent_list_is_newest_first_without_repeats_and_bounded() {
    // `EntryId` has no public constructor: the ids come from a scanned vault.
    let fs = MemFs::new();
    for n in 0..25 {
        fs.insert_file(format!("/vault/{n}.md"), "");
    }
    let vault = futures::executor::block_on(Vault::scan(&fs, Path::new("/vault")))
        .expect("scan the test vault");
    let ids: Vec<EntryId> = vault.entries().map(|entry| entry.id).collect();
    assert_eq!(ids.len(), 25);
    let mut recent = Vec::new();
    for id in &ids {
        remember(&mut recent, *id);
    }
    remember(&mut recent, ids[10]);

    assert_eq!(recent.len(), RECENT);
    assert_eq!(recent[0], ids[10]);
    assert_eq!(recent[1], ids[24]);
    assert_eq!(recent.iter().filter(|id| **id == ids[10]).count(), 1);
}

/// Zed's picker waits for the pending query before it confirms (`picker.rs` `confirm_on_update`).
#[gpui::test]
fn enter_right_after_typing_opens_the_typed_match_not_the_stale_row(cx: &mut TestAppContext) {
    let (window, mut visual, [a, _b, _c]) = three_shown(cx);
    visual.simulate_keystrokes("cmd-p");

    // The MRU lists b first; `a` has not answered when `enter` lands.
    visual.simulate_keystrokes("a enter");

    assert_eq!(showing(&window, &mut visual), Some(a));
}

/// `cmd-p enter` switches to the previous note in Zed, however fast it is typed.
#[gpui::test]
fn cmd_p_then_enter_at_once_opens_the_previous_note(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b, _c]) = three_shown(cx);

    visual.simulate_keystrokes("cmd-p enter");

    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(!has_modal(window, &mut visual));
}

/// A slow note clicked in the tree, then `cmd-p` and a query: the load's focus move must not
/// send the rest of the query into the note.
#[gpui::test]
fn a_load_landing_while_the_picker_is_open_does_not_take_its_keystrokes(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, &mut visual, b);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("a");
    gate.release();
    visual.run_until_parked();
    visual.simulate_keystrokes("l p");

    let text = editor(&window, &mut visual).update(&mut visual, |editor, cx| {
        editor.buffer().read(cx).text().to_string()
    });
    assert_eq!(text, "beta", "the query went into the note");
    assert!(has_modal(window, &mut visual));
}

#[gpui::test]
fn escape_after_a_load_landed_under_the_picker_focuses_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, &mut visual, b);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-p");
    gate.release();
    visual.run_until_parked();
    assert!(
        has_modal(window, &mut visual),
        "the load left the picker open"
    );
    visual.simulate_keystrokes("escape");

    let editor_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(
        editor_focused,
        "the tree was focused before the load moved it"
    );
}

/// Picking the note showing while another note loads: the pick is the user's last word.
#[gpui::test]
fn picking_the_note_showing_during_a_slow_load_keeps_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, &mut visual, b);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-p");
    visual.simulate_keystrokes("a");
    visual.simulate_keystrokes("enter");
    gate.release();
    visual.run_until_parked();

    assert_eq!(showing(&window, &mut visual), Some(a));
}

/// Latent until a file watcher lands: today only the focused tree trashes. Without a picker the
/// same trash leaves focus on the dropped editor too.
#[gpui::test]
fn escape_after_the_note_showing_was_trashed_leaves_the_shell_reachable(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, _b, c]) = three_shown(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .expect("the shell is open");
    visual.simulate_keystrokes("cmd-p");

    let trashed = notes.update(&mut visual, |notes, cx| notes.trash(c, cx));
    visual.run_until_parked();
    drop(trashed);
    assert_eq!(showing(&window, &mut visual), None);
    visual.simulate_keystrokes("escape");

    // Focus went back to the dropped editor's handle: no key reaches `AppShell` any more.
    visual.simulate_keystrokes("cmd-p");
    assert!(has_modal(window, &mut visual), "cmd-p reaches the shell");
}

#[gpui::test]
fn trashing_the_note_showing_while_it_has_focus_leaves_the_shell_reachable(
    cx: &mut TestAppContext,
) {
    let (window, mut visual, [_a, _b, c]) = three_shown(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .expect("the shell is open");

    let trashed = notes.update(&mut visual, |notes, cx| notes.trash(c, cx));
    visual.run_until_parked();
    drop(trashed);
    assert_eq!(showing(&window, &mut visual), None);

    visual.simulate_keystrokes("cmd-p");
    assert!(has_modal(window, &mut visual), "cmd-p reaches the shell");
}
