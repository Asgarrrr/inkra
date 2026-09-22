//! Creating a note or a folder, through the sidebar's field.

use std::sync::Arc;

use gpui::{Focusable, Modifiers, TestAppContext, point, px};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::sidebar::row_center;
use crate::test_support::entry_id;

use super::{activate, open_vault, tree_focused, two_notes, vault_names};

/// `cmd-n` while the **editor** holds focus — the placement test. See
/// CLAUDE.md on why the handler is on `AppShell`, not the tree.
#[gpui::test]
fn cmd_n_opens_a_field_with_the_editor_focused(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    // Put focus on the editor, which is the state the tree cannot reach.
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            let handle = shell.focus_handle(cx);
            window.focus(&handle, cx);
        })
        .unwrap();
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();

    assert!(
        sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
        "cmd-n must reach the tree from the editor"
    );
}

#[gpui::test]
fn typing_a_name_and_confirming_creates_the_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    visual.simulate_keystrokes("p l a n enter");
    visual.run_until_parked();

    assert_eq!(
        vault_names(&notes, cx),
        ["a.md", "b.md", "plan.md"],
        "the note exists, with the extension the name rules add"
    );
    assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}

/// The created row is selected, so the next gesture acts on what was just
/// made rather than on whatever was selected before.
#[gpui::test]
fn the_created_note_becomes_the_selection(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n p l a n enter");
    visual.run_until_parked();

    let made = entry_id(&notes, cx, "plan");
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(made)
    );
}

#[gpui::test]
fn cmd_shift_n_creates_a_folder_rather_than_a_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-shift-n");
    visual.run_until_parked();
    visual.simulate_keystrokes("d o c s enter");
    visual.run_until_parked();

    assert_eq!(
        vault_names(&notes, cx),
        ["docs", "a.md", "b.md"],
        "a folder, with no extension, and sorted before the notes"
    );
}

/// Refused, not silently ignored: the field stays open so the name can be
/// fixed — see `edit::Rejection`.
#[gpui::test]
fn a_duplicate_name_is_refused_and_the_field_stays_open(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n a enter");
    visual.run_until_parked();

    assert_eq!(
        vault_names(&notes, cx),
        ["a.md", "b.md"],
        "nothing was made"
    );
    assert!(
        sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
        "the field stays open so the name can be corrected"
    );
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
        Some("That name is already taken.")
    );
}

/// `plan` and `plan.md` are one name: the check compares the file name the
/// note would actually get, not the text typed.
#[gpui::test]
fn a_duplicate_is_caught_through_the_extension(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n a . m d enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}

#[gpui::test]
fn an_invalid_name_is_refused_with_its_own_reason(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n a / b enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
        Some("A name cannot contain / or :.")
    );
}

#[gpui::test]
fn escape_abandons_the_field_and_makes_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n p l a n escape");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
    assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}

/// A folder selected takes the new note inside it, and is expanded so the
/// field is drawn somewhere visible.
#[gpui::test]
fn a_note_created_under_a_selected_folder_lands_inside_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/projects/existing.md", "e");
    fs.insert_file("/vault/a.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);

    // Row 0 is the `projects` folder.
    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-n p l a n enter");
    visual.run_until_parked();

    assert!(
        vault_names(&notes, cx).contains(&"projects/plan.md".to_owned()),
        "got {:?}",
        vault_names(&notes, cx)
    );
}
