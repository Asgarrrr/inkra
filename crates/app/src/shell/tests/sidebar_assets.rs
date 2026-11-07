use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, Subscription, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::{AppShell, focus_tree, keys, open, selected, shell_focused, showing};
use crate::open::FileActivated;
use crate::sidebar::Sidebar;
use crate::test_support::entry_id;

const PHOTO: &str = "/vault/photo.png";

struct AssetTree {
    fs: Arc<MemFs>,
    window: WindowHandle<AppShell>,
    cx: VisualTestContext,
    a: EntryId,
    photo: EntryId,
    sidebar: Entity<Sidebar>,
    activated: Rc<RefCell<Vec<FileActivated>>>,
    _activated: Subscription,
}

/// `a.md` showing, `photo.png` and `report.pdf` beside it, and a save's leftover temp file.
fn asset_tree(cx: &mut TestAppContext) -> AssetTree {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_bytes(PHOTO, b"\x89PNG".to_vec());
    fs.insert_bytes("/vault/report.pdf", b"%PDF".to_vec());
    fs.insert_file("/vault/a.md.inkra-1-2.tmp", "half a save");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = entry_id(&notes, &mut cx, "a");
    let photo = entry_id(&notes, &mut cx, "photo.png");
    open(&window, &mut cx, a);
    let sidebar = window
        .update(&mut cx, |shell, _, _| shell.panes().sidebar.clone())
        .expect("the shell is open");
    let activated = Rc::new(RefCell::new(Vec::new()));
    let subscription = cx.update(|_, cx| {
        let activated = Rc::clone(&activated);
        cx.subscribe(&sidebar, move |_, event: &FileActivated, _| {
            activated.borrow_mut().push(*event);
        })
    });
    AssetTree {
        fs,
        window,
        cx,
        a,
        photo,
        sidebar,
        activated,
        _activated: subscription,
    }
}

impl AssetTree {
    fn tree(&mut self) -> Vec<String> {
        self.window
            .update(&mut self.cx, |shell, _, cx| shell.tree_row_names(cx))
            .expect("the shell is open")
    }

    fn editing(&mut self) -> bool {
        self.sidebar
            .read_with(&self.cx, |sidebar, _| sidebar.is_editing())
    }

    /// The asset was never read, as a note or as bytes, no note was asked for, and `a` still shows.
    fn assert_inert(&mut self) {
        assert_eq!(self.fs.loads(Path::new(PHOTO)), 0);
        assert_eq!(self.fs.reads(Path::new(PHOTO)), 0);
        assert!(self.activated.borrow().is_empty());
        assert_eq!(showing(&self.window, &mut self.cx), Some(self.a));
    }
}

#[gpui::test]
fn the_tree_lists_assets_and_hides_a_leftover_temp_file(cx: &mut TestAppContext) {
    let mut assets = asset_tree(cx);

    assert_eq!(assets.tree(), ["a", "photo.png", "report.pdf"]);
}

#[gpui::test]
fn enter_on_an_asset_opens_its_name_field_and_no_note(cx: &mut TestAppContext) {
    let mut assets = asset_tree(cx);
    focus_tree(&assets.window, &mut assets.cx);
    keys(&mut assets.cx, "down");

    keys(&mut assets.cx, "enter");

    assert!(assets.editing());
    assert_eq!(selected(&assets.window, &mut assets.cx), Some(assets.photo));
    assets.assert_inert();
}

/// The name field holds the whole name, so a rename keeps the extension.
#[gpui::test]
fn renaming_an_asset_keeps_its_extension(cx: &mut TestAppContext) {
    let mut assets = asset_tree(cx);
    focus_tree(&assets.window, &mut assets.cx);
    keys(&mut assets.cx, "down");

    keys(&mut assets.cx, "enter");
    keys(&mut assets.cx, "left x enter");

    assert_eq!(assets.tree(), ["a", "report.pdf", "xphoto.png"]);
    assert_eq!(assets.fs.loads(Path::new("/vault/xphoto.png")), 0);
    assert_eq!(assets.fs.reads(Path::new("/vault/xphoto.png")), 0);
    assets.assert_inert();
}

/// `photo.png` renamed `a.MD` would become a note, so it collides with `a.md`.
#[gpui::test]
fn renaming_an_asset_to_a_taken_note_name_keeps_the_field_open(cx: &mut TestAppContext) {
    let mut assets = asset_tree(cx);
    focus_tree(&assets.window, &mut assets.cx);
    keys(&mut assets.cx, "down");

    keys(&mut assets.cx, "enter cmd-a");
    assets.cx.simulate_input("a.MD");
    keys(&mut assets.cx, "enter");

    assert!(assets.editing());
    assert_eq!(
        assets
            .sidebar
            .read_with(&assets.cx, |sidebar, _| sidebar.rejection_message()),
        Some("That name is already taken.")
    );
    assert_eq!(assets.tree(), ["a", "photo.png", "report.pdf"]);
}

/// Trash takes notes only.
#[gpui::test]
fn cmd_backspace_on_an_asset_asks_nothing_and_trashes_nothing(cx: &mut TestAppContext) {
    let mut assets = asset_tree(cx);
    focus_tree(&assets.window, &mut assets.cx);
    keys(&mut assets.cx, "down");

    keys(&mut assets.cx, "cmd-backspace");

    assert!(assets.cx.pending_prompt().is_none());
    assert_eq!(assets.fs.trashed(Path::new(PHOTO)), None);
    assets.assert_inert();
}
