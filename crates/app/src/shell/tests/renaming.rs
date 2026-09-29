use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Modifiers, TestAppContext, point, px};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::sidebar::row_center;
use crate::test_support::entry_id;
use crate::vault::{NoteStore, RenameFailed};

use super::{activate, open_vault, tree_focused, two_notes, vault_names};

/// A rename keeps the row's `EntryId`, so any open buffer stays bound.
#[gpui::test]
fn renaming_a_note_moves_it_and_keeps_its_id(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("down");
    visual.run_until_parked();
    let before = sidebar
        .read_with(cx, |sidebar, _| sidebar.selected())
        .unwrap();

    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("p l a n enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["b.md", "plan.md"]);
    assert_eq!(
        entry_id(&notes, cx, "plan"),
        before,
        "the inode did not move, so neither did the id"
    );
}

/// The write must land on the old path before the rename moves it, or the
/// write recreates the note under its old name.
#[gpui::test]
async fn a_rename_issued_during_a_write_moves_the_written_text(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);

    activate(&sidebar, cx, a);
    visual.run_until_parked();
    let buffer = notes
        .read_with(cx, |notes, _| notes.buffer_for(a).cloned())
        .expect("the note is open");

    let gate = fs.block_save("/vault/a.md");
    buffer.update(cx, |buffer, _| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(inkra_text::ByteOffset(0), "EDITED ");
        });
    });
    notes.update(cx, NoteStore::save_current);
    visual.run_until_parked();

    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "plan", cx));
    visual.run_until_parked();
    gate.release();
    assert!(renamed.await, "the rename lands once the write has");
    visual.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/plan.md")).as_deref(),
        Some("EDITED alpha"),
        "the write reached disk, and the rename moved what it wrote"
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "nothing was recreated under the old name"
    );
}

/// Does not pin the rescan itself, only that the buffer survives the move
/// and its next write follows the file.
#[gpui::test]
async fn a_note_edited_after_a_rename_is_written_to_its_new_path(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);

    activate(&sidebar, cx, a);
    visual.run_until_parked();
    assert!(
        notes
            .update(cx, |notes, cx| notes.rename(a, "plan", cx))
            .await
    );
    visual.run_until_parked();

    let buffer = notes
        .read_with(cx, |notes, _| notes.buffer_for(a).cloned())
        .expect("the note is still open under the same id");
    buffer.update(cx, |buffer, _| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(inkra_text::ByteOffset(0), "AFTER ");
        });
    });
    notes.update(cx, NoteStore::save_current);
    visual.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/plan.md")).as_deref(),
        Some("AFTER alpha")
    );
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
}

#[gpui::test]
fn renaming_onto_a_sibling_name_is_refused(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("down enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("b enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"], "neither moved");
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
        Some("That name is already taken.")
    );
}

#[gpui::test]
fn renaming_a_note_to_its_own_name_is_accepted(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("down enter enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
    assert!(
        !sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
        "the field closed rather than reporting a duplicate"
    );
}

/// `a#b` is a name `check_name` refuses, but confirming it unchanged renames nothing.
#[gpui::test]
fn confirming_an_unchanged_refused_name_closes_the_field(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a#b.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("down enter enter");
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a#b.md"]);
    assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}

#[gpui::test]
fn renaming_replaces_the_row_rather_than_adding_one(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    let before = sidebar.read_with(cx, |sidebar, _| sidebar.item_count());
    visual.simulate_keystrokes("down enter");
    visual.run_until_parked();

    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.item_count()),
        before
    );
}

#[gpui::test]
fn renaming_a_folder_moves_its_subtree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/projects/plan.md", "p");
    fs.insert_file("/vault/a.md", "alpha");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("a r c h i v e enter");
    visual.run_until_parked();

    assert_eq!(
        vault_names(&notes, cx),
        ["archive", "archive/plan.md", "a.md"]
    );
}

/// Both resolve their path inside the chain link, not at issue time, or the
/// delete looks for a name that has already moved.
#[gpui::test]
async fn a_delete_queued_behind_a_rename_still_finds_the_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);

    let (renamed, trashed) = notes.update(cx, |notes, cx| {
        (notes.rename(a, "plan", cx), notes.trash(a, cx))
    });
    visual.run_until_parked();

    assert!(renamed.await, "the rename lands first");
    assert!(
        trashed.await,
        "and the delete finds the note under its new name"
    );
    assert_eq!(vault_names(&notes, cx), ["b.md"]);
    assert_eq!(
        fs.trashed(Path::new("/vault/plan.md")).as_deref(),
        Some("alpha")
    );
}

#[gpui::test]
async fn two_renames_in_one_turn_both_land(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);

    let (first, second) = notes.update(cx, |notes, cx| {
        (notes.rename(a, "plan", cx), notes.rename(a, "final", cx))
    });
    visual.run_until_parked();

    assert!(first.await);
    assert!(
        second.await,
        "the second rename resolves the path the first produced"
    );
    assert_eq!(vault_names(&notes, cx), ["b.md", "final.md"]);
}

#[gpui::test]
async fn a_failed_rename_is_reported(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);
    fs.make_uncreatable("/vault/plan.md");

    let failed = Rc::new(RefCell::new(Vec::new()));
    let _guard = cx.update(|cx| {
        let failed = Rc::clone(&failed);
        cx.subscribe(&notes, move |_, event: &RenameFailed, _| {
            failed.borrow_mut().push(*event);
        })
    });

    assert!(
        !notes
            .update(cx, |notes, cx| notes.rename(a, "plan", cx))
            .await
    );
    visual.run_until_parked();

    assert_eq!(*failed.borrow(), [RenameFailed(a)]);
    assert_eq!(
        vault_names(&notes, cx),
        ["a.md", "b.md"],
        "the note did not move"
    );
}

/// `current` can be `DETACHED`; a caller must not move a file the vault does not list.
#[gpui::test]
async fn renaming_the_command_line_note_does_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);

    assert!(
        !notes
            .update(cx, |notes, cx| notes.rename(EntryId::DETACHED, "plan", cx))
            .await
    );
    visual.run_until_parked();

    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
}

/// While a name is typed, the tree's own keys must reach no handler.
#[gpui::test]
fn the_trees_keys_do_nothing_while_a_name_is_typed(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("down enter");
    visual.run_until_parked();
    let selected = sidebar.read_with(cx, |sidebar, _| sidebar.selected());

    visual.simulate_keystrokes("down");
    visual.run_until_parked();
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        selected,
        "an arrow must not move the selection out from under the field"
    );

    visual.simulate_keystrokes("cmd-backspace");
    visual.run_until_parked();
    assert!(
        !cx.has_pending_prompt(),
        "cmd-backspace edits the name; it must not offer to delete the row"
    );
    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}

#[gpui::test]
fn the_remaining_bindings_fire(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("up");
    visual.run_until_parked();
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(entry_id(&notes, cx, "b"))
    );
    visual.simulate_keystrokes("up");
    visual.run_until_parked();
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
        Some(entry_id(&notes, cx, "a")),
        "up moves the selection towards the top"
    );

    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("home delete");
    visual.run_until_parked();
    visual.simulate_keystrokes("end shift-left X");
    visual.run_until_parked();
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();

    assert!(
        vault_names(&notes, cx).contains(&"X.md".to_owned()),
        "home, delete, end and shift-left all reached the field: got {:?}",
        vault_names(&notes, cx)
    );
}
