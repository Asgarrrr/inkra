//! An image in a tab: the open gate, the load, the eviction (file kinds spec).

use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, Modifiers, RenderImage, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::dragging::drag;
use super::splits::{centre, close, split};
use super::{
    AppShell, at, check, click_row, image_shown, keys, open, select, shell_focused, showing,
};
use super::{tab_files, view_of};
use crate::images::{Slot, png};
use crate::test_support::entry_id;
use crate::vault::NoteStore;

pub(super) const PHOTO: &str = "/vault/photo.png";
pub(super) const PDF: &str = "/vault/report.pdf";
/// Tree rows: `a`, `photo.png`, `report.pdf`.
const PHOTO_ROW: f32 = 1.;
const PDF_ROW: f32 = 2.;

pub(super) struct Vault {
    pub(super) fs: Arc<MemFs>,
    pub(super) notes: Entity<NoteStore>,
    pub(super) window: WindowHandle<AppShell>,
    pub(super) cx: VisualTestContext,
    pub(super) a: EntryId,
    pub(super) photo: EntryId,
}

/// `a.md` showing, `photo.png` holding `photo` and `report.pdf` beside it.
pub(super) fn vault(cx: &mut TestAppContext, photo: Vec<u8>) -> Vault {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_bytes(PHOTO, photo);
    fs.insert_bytes(PDF, b"%PDF".to_vec());
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = entry_id(&notes, &mut cx, "a");
    let photo = entry_id(&notes, &mut cx, "photo.png");
    open(&window, &mut cx, a);
    Vault {
        fs,
        notes,
        window,
        cx,
        a,
        photo,
    }
}

impl Vault {
    pub(super) fn click_photo(&mut self) {
        click_row(&mut self.cx, PHOTO_ROW, Modifiers::none());
    }

    pub(super) fn image_shown(&mut self) -> Option<EntryId> {
        image_shown(&self.window, &mut self.cx)
    }

    pub(super) fn drawn(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }

    fn reads(&self) -> usize {
        self.fs.reads(Path::new(PHOTO))
    }

    pub(super) fn held(&mut self) -> bool {
        let photo = self.photo;
        self.window
            .update(&mut self.cx, |shell, _, cx| {
                shell.panes().images.read(cx).slot(photo).is_some()
            })
            .unwrap()
    }

    fn ready(&mut self) -> Arc<RenderImage> {
        let photo = self.photo;
        self.window
            .update(&mut self.cx, |shell, _, cx| {
                match shell.panes().images.read(cx).slot(photo) {
                    Some(Slot::Ready(image)) => Arc::clone(image),
                    _ => panic!("the image is ready"),
                }
            })
            .unwrap()
    }

    fn in_atlas(&mut self, image: &RenderImage) -> bool {
        self.window
            .update(&mut self.cx, |_, window, _| {
                window.has_image_atlas_entry(image)
            })
            .unwrap()
    }

    fn titles(&mut self) -> Vec<String> {
        self.window
            .update(&mut self.cx, |shell, _, cx| {
                let titles = shell.tab_titles(shell.layout.last_center(), cx);
                titles.into_iter().map(String::from).collect()
            })
            .unwrap()
    }

    pub(super) fn check(&mut self) {
        check(&self.window, &mut self.cx);
    }
}

#[gpui::test]
fn a_png_clicked_in_the_tree_opens_in_the_tab(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));

    vault.click_photo();

    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert_eq!(vault.titles(), ["photo.png"]);
    assert!(vault.drawn("image-view"));
    assert!(!vault.drawn("image-failed"));
    vault.check();
}

#[gpui::test]
fn a_pdf_clicked_in_the_tree_opens_nothing(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));

    click_row(&mut vault.cx, PDF_ROW, Modifiers::none());

    assert_eq!(showing(&vault.window, &mut vault.cx), Some(vault.a));
    assert_eq!(vault.fs.loads(Path::new(PDF)), 0);
    assert_eq!(vault.fs.reads(Path::new(PDF)), 0);
    vault.check();
}

#[gpui::test]
fn an_image_not_loaded_yet_shows_at_once_then_lands(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    let gate = vault.fs.block_read_bytes(PHOTO);

    vault.click_photo();
    let shown = vault.image_shown();
    let drawn = vault.drawn("image-view");
    gate.release();
    vault.cx.run_until_parked();

    assert_eq!(shown, Some(vault.photo));
    assert!(!drawn, "nothing is drawn before the decode lands");
    assert!(vault.drawn("image-view"));
    vault.check();
}

#[gpui::test]
fn an_unreadable_image_shows_the_message(cx: &mut TestAppContext) {
    let mut vault = vault(cx, b"not an image".to_vec());

    vault.click_photo();

    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert!(vault.drawn("image-failed"));
    assert!(!vault.drawn("image-view"));
    vault.check();
}

/// The failure is about its path: a vault change that leaves the path alone reads nothing.
#[gpui::test]
fn an_unreadable_image_is_not_read_again_when_another_file_changes(cx: &mut TestAppContext) {
    let mut vault = vault(cx, b"not an image".to_vec());
    vault.click_photo();

    vault.fs.insert_file("/vault/b.md", "beta");
    vault.cx.run_until_parked();

    assert_eq!(vault.reads(), 1);
    assert!(vault.drawn("image-failed"));
    vault.check();
}

#[gpui::test]
fn an_image_no_group_shows_is_evicted_from_the_atlas(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    keys(&mut vault.cx, "cmd-t");
    vault.click_photo();
    let image = vault.ready();
    let while_shown = vault.in_atlas(&image);

    keys(&mut vault.cx, "ctrl-tab");

    assert!(while_shown);
    assert!(!vault.in_atlas(&image));
    assert!(!vault.held());
    vault.check();
}

#[gpui::test]
fn moving_an_image_tab_reads_it_once(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    split(&mut vault.cx);
    keys(&mut vault.cx, "cmd-t");
    vault.click_photo();

    drag(&mut vault.cx, "tab-1-1", "tab-0");

    let groups = centre(&vault.window, &mut vault.cx);
    let last = vault
        .window
        .update(&mut vault.cx, |shell, _, _| shell.layout.last_center())
        .unwrap();
    assert_eq!(last, groups[0]);
    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert_eq!(vault.reads(), 1);
    assert!(vault.drawn("image-view"));
    vault.check();
    close(&mut vault.cx);
}

#[gpui::test]
fn a_split_image_stays_loaded_until_both_groups_close_it(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    vault.click_photo();
    split(&mut vault.cx);
    assert_eq!(vault.image_shown(), Some(vault.photo), "the split shows it");

    keys(&mut vault.cx, "cmd-w");
    let after_one = vault.held();
    vault.check();
    keys(&mut vault.cx, "cmd-w");

    assert!(after_one, "the first group still shows it");
    assert!(!vault.held());
    assert_eq!(vault.reads(), 1);
    vault.check();
}

#[gpui::test]
fn showing_an_image_records_no_recent(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));

    vault.click_photo();

    let recent = vault
        .window
        .update(&mut vault.cx, |shell, _, _| shell.recent.clone())
        .unwrap();
    assert_eq!(recent, [vault.a]);
}

#[gpui::test]
fn cmd_w_closes_an_image_tab(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    keys(&mut vault.cx, "cmd-t");
    vault.click_photo();

    keys(&mut vault.cx, "cmd-w");

    assert_eq!(
        tab_files(&vault.window, &mut vault.cx),
        (vec![Some(vault.a)], 0)
    );
    assert!(!vault.held());
    vault.check();
}

#[gpui::test]
fn cmd_shift_t_reopens_a_closed_image_tab_and_shows_the_image(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    keys(&mut vault.cx, "cmd-t");
    vault.click_photo();
    keys(&mut vault.cx, "cmd-w");

    keys(&mut vault.cx, "cmd-shift-t");

    assert_eq!(
        tab_files(&vault.window, &mut vault.cx),
        (vec![Some(vault.a), Some(vault.photo)], 1)
    );
    assert_eq!(vault.image_shown(), Some(vault.photo));
    assert!(vault.drawn("image-view"));
    vault.check();
}

#[gpui::test]
fn cmd_bracket_left_goes_back_from_an_image_to_the_note_and_its_selection(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    select(&vault.window, &mut vault.cx, 1..3);
    vault.click_photo();

    keys(&mut vault.cx, "cmd-[");

    assert_eq!(showing(&vault.window, &mut vault.cx), Some(vault.a));
    assert_eq!(view_of(&vault.window, &mut vault.cx).0, at(1)..at(3));
    assert!(!vault.held());
    vault.check();
}

#[gpui::test]
fn cmd_t_with_an_image_shown_opens_an_empty_tab(cx: &mut TestAppContext) {
    let mut vault = vault(cx, png(40, 30));
    vault.click_photo();

    keys(&mut vault.cx, "cmd-t");

    assert_eq!(
        tab_files(&vault.window, &mut vault.cx),
        (vec![Some(vault.photo), None], 1)
    );
    assert!(!vault.held());
    vault.check();
}
