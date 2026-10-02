use std::sync::Arc;

use gpui::{Focusable, Modifiers, TestAppContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::test_support::entry_id;

use super::{activate, click_row, no_vault, open_vault, tree_focused, two_notes, vault_names};

#[gpui::test]
fn cmd_n_opens_a_field_with_the_editor_focused(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

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

/// The check compares the file name the note would get, not the text typed.
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
fn a_note_with_an_uppercase_extension_takes_its_name(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/plan.MD", "plan");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n p l a n enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["plan.MD"]);
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
        Some("That name is already taken.")
    );
}

#[gpui::test]
fn a_name_a_wikilink_cannot_spell_is_refused(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-n a # b enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
        Some("A name cannot contain #, |, ^, [ or ].")
    );
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

#[gpui::test]
fn a_note_created_under_a_selected_folder_lands_inside_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/projects/existing.md", "e");
    fs.insert_file("/vault/a.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);

    click_row(&mut visual, 0., Modifiers::none());
    visual.simulate_keystrokes("cmd-n p l a n enter");
    visual.run_until_parked();

    assert!(
        vault_names(&notes, cx).contains(&"projects/plan.md".to_owned()),
        "got {:?}",
        vault_names(&notes, cx)
    );
}

/// `g` sits between the note and its folder: the parent is the nearest folder one level up.
#[gpui::test]
fn a_note_created_beside_a_selected_note_lands_in_its_folder(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/f/g/x.md", "");
    fs.insert_file("/vault/f/a.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);
    let a = entry_id(&notes, cx, "a");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.toggle_named("f", cx);
        sidebar.select_for_test(a, cx);
    });
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-n p l a n enter");
    visual.run_until_parked();

    assert!(
        vault_names(&notes, cx).contains(&"f/plan.md".to_owned()),
        "got {:?}",
        vault_names(&notes, cx)
    );
}

/// Opening scrolls the list to the field: an undrawn field swallows every keystroke.
#[gpui::test]
fn a_field_opened_far_below_the_drawn_rows_takes_keystrokes(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    // Zero-padded so the comparator's lexical order is the numeric one.
    for ix in 0..200 {
        fs.insert_file(format!("/vault/note-{ix:03}.md"), "text");
    }
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);
    let last = entry_id(&notes, cx, "note-199");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last, cx));
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-n");
    visual.run_until_parked();
    visual.simulate_keystrokes("z enter");
    visual.run_until_parked();

    assert!(vault_names(&notes, cx).contains(&"z.md".to_owned()));
}

#[gpui::test]
fn cmd_n_does_nothing_without_a_vault(cx: &mut TestAppContext) {
    assert_no_name_field_after("cmd-n", cx);
}

#[gpui::test]
fn cmd_shift_n_does_nothing_without_a_vault(cx: &mut TestAppContext) {
    assert_no_name_field_after("cmd-shift-n", cx);
}

fn assert_no_name_field_after(keys: &str, cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx);

    visual.simulate_keystrokes(keys);
    visual.run_until_parked();

    let root_focused = window
        .update(&mut visual, |shell, window, _| {
            shell.focus_handle.is_focused(window)
        })
        .expect("the shell is open");
    assert!(root_focused, "no name field may take the keys");
}
