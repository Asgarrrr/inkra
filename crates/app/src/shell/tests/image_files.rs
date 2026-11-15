//! An image whose file is deleted or renamed: its tab, and the path its read follows.

use std::path::Path;

use gpui::{Modifiers, Task, TestAppContext};
use inkra_vault::EntryId;

use super::images::{PDF, PHOTO, Vault, vault};
use super::{click_row, keys, showing, tab_files};
use crate::images::png;

impl Vault {
    fn rename_photo(&mut self, name: &str) -> Task<bool> {
        let photo = self.photo;
        self.notes
            .update(&mut self.cx, |notes, cx| notes.rename(photo, name, cx))
    }

    /// An outside rename: the same file under `to`, so it keeps its id.
    fn rename_outside(&mut self, from: &str, to: &str) {
        self.fs.hard_link(Path::new(from), to);
        self.fs.remove(Path::new(from));
        self.cx.run_until_parked();
    }
}

#[gpui::test]
fn a_deleted_image_empties_its_tab(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    keys(&mut vault.cx, "cmd-t");
    vault.click_photo();

    vault.fs.remove(Path::new(PHOTO));
    vault.cx.run_until_parked();

    assert_eq!(
        tab_files(&vault.window, &mut vault.cx),
        (vec![Some(vault.a), None], 1)
    );
    assert!(!vault.held());
    vault.check();
}

#[gpui::test]
fn a_deleted_image_in_history_only_is_forgotten(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    vault.click_photo();
    click_row(&mut vault.cx, 0., Modifiers::none());

    vault.fs.remove(Path::new(PHOTO));
    vault.cx.run_until_parked();

    let files: Vec<EntryId> = vault
        .window
        .update(&mut vault.cx, |shell, _, _| {
            shell.center().tabs.files().collect()
        })
        .unwrap();
    assert!(!files.contains(&vault.photo));
    assert_eq!(showing(&vault.window, &mut vault.cx), Some(vault.a));
    vault.check();
}

/// The walk after the first deletion records the vault's `departures`; the second must renew it.
#[gpui::test]
fn an_image_deleted_after_an_unheld_file_is_forgotten(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    vault.click_photo();
    click_row(&mut vault.cx, 0., Modifiers::none());
    vault.fs.remove(Path::new(PDF));
    vault.cx.run_until_parked();

    vault.fs.remove(Path::new(PHOTO));
    vault.cx.run_until_parked();

    let files: Vec<EntryId> = vault
        .window
        .update(&mut vault.cx, |shell, _, _| {
            shell.center().tabs.files().collect()
        })
        .unwrap();
    assert!(!files.contains(&vault.photo));
    vault.check();
}

/// The open editor stays on the note; the next display shows the file as an image.
#[gpui::test]
fn a_note_renamed_to_a_png_under_its_tab_shows_as_an_image_once_shown_again(
    cx: &mut TestAppContext,
) {
    let mut vault = vault(cx, png(40, 30));

    vault.rename_outside("/vault/a.md", "/vault/a.png");
    let still_bound = showing(&vault.window, &mut vault.cx);
    vault.check();
    keys(&mut vault.cx, "cmd-t ctrl-tab");

    assert_eq!(still_bound, Some(vault.a));
    assert_eq!(vault.image_shown(), Some(vault.a));
    assert!(vault.drawn("image-failed"), "a note's text is no image");
    vault.check();
}

/// The image stays drawn; the next display has no buffer for the note, so it loads the file's bytes, which still decode.
#[gpui::test]
fn a_png_renamed_to_a_note_under_its_tab_still_shows_the_image_once_shown_again(
    cx: &mut TestAppContext,
) {
    let mut vault = vault(cx, png(40, 30));
    vault.click_photo();

    vault.rename_outside(PHOTO, "/vault/photo.md");
    let still_drawn = vault.drawn("image-view");
    vault.check();
    keys(&mut vault.cx, "cmd-t ctrl-tab");

    assert!(still_drawn);
    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert!(vault.drawn("image-view"));
    vault.check();
}

/// The read follows the rename's move, not the vault's stale path, so the image still draws once the rescan lands.
#[gpui::test]
fn an_image_opened_before_its_rename_rescan_lands_is_drawn(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    let gate = vault.fs.block_read_dir("/vault");
    let _renamed = vault.rename_photo("shot.png");
    vault.cx.run_until_parked();

    vault.click_photo();
    gate.release();
    vault.cx.run_until_parked();

    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert!(vault.drawn("image-view"));
    assert!(!vault.drawn("image-failed"));
    vault.check();
}

/// Opened while `Fs::rename` runs: no path is safe yet, so the read waits behind the rename.
#[gpui::test]
fn an_image_opened_while_its_rename_runs_is_drawn(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    let rescan = vault.fs.block_read_dir("/vault");
    let rename = vault.fs.block_after_rename(PHOTO);
    let _renamed = vault.rename_photo("shot.png");
    vault.cx.run_until_parked();

    vault.click_photo();
    rename.release();
    rescan.release();
    vault.cx.run_until_parked();

    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert!(vault.drawn("image-view"));
    assert!(!vault.drawn("image-failed"));
    vault.check();
}
