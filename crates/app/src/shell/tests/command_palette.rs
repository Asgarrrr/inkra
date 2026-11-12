use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Side, View};
use inkra_vault::mem_fs::MemFs;

use super::{activate, editor, no_vault, showing, tab_files, tree_focused, two_notes, vault_names};
use crate::Quit;
use crate::shell::{AppShell, NoteStore};
use crate::sidebar::Sidebar;

/// Note `a` (`alpha`) open; `open_note` leaves the editor focused.
fn editor_focused(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<AppShell>,
    VisualTestContext,
    Entity<Sidebar>,
    Entity<NoteStore>,
) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);
    activate(&sidebar, cx, a);
    visual.run_until_parked();
    (window, visual, sidebar, notes)
}

fn names(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    window
        .update(cx, |shell, _, cx| shell.palette_names(cx))
        .unwrap()
}

fn listed(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<String> {
    names(window, cx).expect("the palette is open")
}

/// A row label's name, without the key that may follow it.
fn name(label: &str) -> &str {
    label.split_once(", ").map_or(label, |(name, _)| name)
}

fn has(names: &[String], wanted: &str) -> bool {
    names.iter().any(|label| name(label) == wanted)
}

fn text(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> String {
    let editor = editor(window, cx);
    editor.read_with(cx, |editor, cx| editor.buffer().read(cx).text().to_string())
}

#[gpui::test]
fn cmd_shift_p_opens_the_palette_and_closes_it(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_some());
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_none());
}

#[gpui::test]
fn the_editor_lists_its_commands_not_its_caret_moves(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    for shown in ["Undo", "Save", "Select all", "New note", "Quick open"] {
        assert!(has(&names, shown), "{shown} missing from {names:?}");
    }
    for hidden in ["Left", "Up", "Backspace", "Toggle"] {
        assert!(!has(&names, hidden), "{hidden} shown in {names:?}");
    }
}

#[gpui::test]
fn the_palette_lists_both_dock_toggles(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    for shown in ["Toggle left dock", "Toggle right dock"] {
        assert!(has(&names, shown), "{shown} missing from {names:?}");
    }
}

#[gpui::test]
fn the_palette_lists_both_move_actions(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    for shown in ["Move files to other dock", "Move backlinks to other dock"] {
        assert!(has(&names, shown), "{shown} missing from {names:?}");
    }
}

#[gpui::test]
fn choosing_move_files_moves_the_tree_right(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p m o v e space f i l e s enter");
    visual.run_until_parked();
    let side = window
        .update(&mut visual, |shell, _, _| shell.side_of(View::Files))
        .unwrap();
    assert_eq!(side, Side::Right);
}

#[gpui::test]
fn the_tree_lists_its_commands_not_the_editors(cx: &mut TestAppContext) {
    let (window, _visual, sidebar, _) = editor_focused(cx);
    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(has(&names, "Rename"), "{names:?}");
    for hidden in ["Undo", "Select next"] {
        assert!(!has(&names, hidden), "{names:?}");
    }
}

#[gpui::test]
fn the_rename_field_lists_its_commands_not_its_keys(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("down enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(has(&names, "Select all"), "{names:?}");
    assert!(
        names.iter().any(|label| label == "Rename"),
        "a shadowed key is not shown: {names:?}"
    );
    for hidden in ["Confirm", "Cancel"] {
        assert!(!has(&names, hidden), "{names:?}");
    }
    visual.simulate_keystrokes("escape z enter");
    visual.run_until_parked();
    assert_eq!(vault_names(&notes, &mut visual), ["b.md", "z.md"]);
}

/// A note landing under the palette must move neither the command's tree nor the cursor the user arrowed.
#[gpui::test]
fn a_command_runs_where_it_was_listed_though_a_note_lands(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, &mut visual, b);
    visual.run_until_parked();
    visual.simulate_keystrokes("down up");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    gate.release();
    visual.run_until_parked();
    visual.simulate_keystrokes("r e n a m e enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("p l a n enter");
    visual.run_until_parked();
    assert_eq!(vault_names(&notes, &mut visual), ["b.md", "plan.md"]);
    assert_eq!(text(&window, &mut visual), "beta");
}

#[gpui::test]
fn typing_narrows_by_word_start(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p n e w space n o");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["New note, ⌘N"]);
}

#[gpui::test]
fn a_row_shows_its_key(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p s a v e");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["Save, ⌘S"]);
}

#[gpui::test]
fn choosing_new_note_creates_a_note(cx: &mut TestAppContext) {
    let (window, mut visual, _, notes) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p n e w space n o enter");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_none());
    visual.simulate_keystrokes("p l a n enter");
    visual.run_until_parked();
    assert_eq!(
        vault_names(&notes, &mut visual),
        ["a.md", "b.md", "plan.md"]
    );
}

/// "Follow link in new tab" matches too, and sorts first by name; its match starts later.
#[gpui::test]
fn new_tab_ranks_new_tab_first_with_a_note_showing(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p n e w space t a b");
    visual.run_until_parked();
    assert!(has(&listed(window, &mut visual), "Follow link in new tab"));

    visual.simulate_keystrokes("enter");
    visual.run_until_parked();

    let tabs = tab_files(&window, &mut visual).0;
    assert_eq!(
        (tabs.len(), tabs.get(1)),
        (2, Some(&None)),
        "\"New tab\" ran"
    );
}

#[gpui::test]
fn the_palette_lists_go_back_and_go_forward_with_their_keys(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p g o space b a");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["Go back, ⌘["]);

    visual.simulate_keystrokes("cmd-shift-p cmd-shift-p g o space f o");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["Go forward, ⌘]"]);
}

#[gpui::test]
fn the_palette_lists_reopen_closed_tab_with_its_key(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p r e o p");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["Reopen closed tab, ⇧⌘T"]);
}

#[gpui::test]
fn a_recent_command_stays_first_though_its_match_starts_later(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p f o l l o w space n e w enter");
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-shift-p n e w space t a b");
    visual.run_until_parked();

    let names = listed(window, &mut visual);
    assert_eq!(
        names.iter().map(|label| name(label)).collect::<Vec<_>>(),
        ["Follow link in new tab", "New tab"]
    );
}

#[gpui::test]
fn choosing_undo_reaches_the_editor(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("x");
    visual.run_until_parked();
    assert_eq!(text(&window, &mut visual), "alphax");
    visual.simulate_keystrokes("cmd-shift-p u n d o enter");
    visual.run_until_parked();
    assert_eq!(text(&window, &mut visual), "alpha");
}

#[gpui::test]
fn choosing_quick_open_leaves_quick_open_open(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p q u i c k enter");
    visual.run_until_parked();
    let quick_open = window
        .update(&mut visual, |shell, _, cx| shell.quick_open_state(cx))
        .unwrap();
    assert!(
        quick_open.is_some(),
        "the palette's Dismissed closed quick open"
    );
}

#[gpui::test]
fn over_quick_open_it_lists_the_notes_commands(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-p");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(has(&names, "Undo"), "{names:?}");
}

#[gpui::test]
fn opens_with_no_vault(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.on_action(|_: &Quit, _| {});
    });
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(has(&names, "Quit"), "{names:?}");
}

#[gpui::test]
fn the_palette_lists_open_vault(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(has(&names, "Open vault"), "{names:?}");
}

#[gpui::test]
fn the_newest_recent_command_lists_first(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p s a v e enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("x cmd-shift-p u n d o enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert_eq!([name(&names[0]), name(&names[1])], ["Undo", "Save"]);
    assert!(
        names[2..].iter().map(|label| name(label)).is_sorted(),
        "the rest stays alphabetical: {names:?}"
    );
}

/// The list was built for the editor; the note is trashed under the palette, so the command lands on the root.
#[gpui::test]
fn choosing_after_the_listed_note_was_trashed_leaves_the_shell_reachable(cx: &mut TestAppContext) {
    let (window, mut visual, _, notes) = editor_focused(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    let a = showing(&window, &mut visual).expect("a is showing");
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let trashed = notes.update(&mut visual, |notes, cx| notes.trash(a, cx));
    visual.run_until_parked();
    drop(trashed);
    assert_eq!(showing(&window, &mut visual), None);
    visual.simulate_keystrokes("s a v e enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    assert!(
        names(window, &mut visual).is_some(),
        "cmd-shift-p reaches the shell"
    );
}
