use std::path::Path;
use std::sync::Arc;

use gpui::TestAppContext;
use inkra_vault::mem_fs::MemFs;

use super::{activate, editor, tree_focused, two_notes};

#[gpui::test]
fn an_arrow_moves_the_tree_and_not_the_caret_while_the_tree_has_focus(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let caret_before = editor(&window, cx).update(cx, |editor, _| editor.cursor());

    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("down down");

    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(b),
        "the tree must claim the arrow"
    );
    assert_eq!(
        editor(&window, cx).update(cx, |editor, _| editor.cursor()),
        caret_before,
        "and the editor must not also see it"
    );
    let _ = notes;
}

#[gpui::test]
fn cmd_backspace_asks_before_trashing_the_selected_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let mut visual = tree_focused(cx, window, &sidebar);
    // `activate` never sets `selected`; select via the arrow instead.
    visual.simulate_keystrokes("down");
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(a)
    );

    visual.simulate_keystrokes("cmd-backspace");
    visual.run_until_parked();

    let (message, _) = cx.pending_prompt().expect("the delete must ask first");
    assert!(
        message.contains('a'),
        "the prompt names the note: {message:?}"
    );
    cx.simulate_prompt_answer("Move to Trash");
    visual.run_until_parked();

    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
    let _ = notes;
}

#[gpui::test]
fn declining_the_prompt_keeps_the_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("down");
    visual.simulate_keystrokes("cmd-backspace");
    visual.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    visual.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
}

/// A buffer with no file is still typable, so the editor must go with the note.
#[gpui::test]
fn trashing_the_open_note_empties_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    assert!(
        window
            .update(cx, |shell, _, _| shell.editor.is_some())
            .expect("the shell is open"),
        "the note is on screen to begin with"
    );

    notes.update(cx, |notes, cx| notes.trash(a, cx)).detach();
    cx.run_until_parked();

    assert!(
        window
            .update(cx, |shell, _, _| shell.editor.is_none())
            .expect("the shell is open"),
        "the editor must go with the note"
    );
    let _ = sidebar;
}

#[gpui::test]
fn trashing_a_note_that_is_not_on_screen_leaves_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    notes.update(cx, |notes, cx| notes.trash(b, cx)).detach();
    cx.run_until_parked();

    assert!(
        window
            .update(cx, |shell, _, _| shell.editor.is_some())
            .expect("the shell is open"),
        "the editor was showing a different note"
    );
    assert_eq!(fs.read_to_string(Path::new("/vault/b.md")), None);
}

#[gpui::test]
fn the_selection_moves_off_a_deleted_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, b) = two_notes(cx, &fs);

    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("down");
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(a)
    );

    visual.simulate_keystrokes("cmd-backspace");
    visual.run_until_parked();
    cx.simulate_prompt_answer("Move to Trash");
    visual.run_until_parked();

    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(b),
        "the selection follows to the next row"
    );
}
