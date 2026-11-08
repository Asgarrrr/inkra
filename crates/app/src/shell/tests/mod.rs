use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui::{
    Entity, Modifiers, Pixels, Point, Task, TestAppContext, VisualTestContext, WindowHandle,
};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::{Gate, MemFs};
use inkra_vault::{EntryId, Fs};

use crate::keymap::key_bindings;
use crate::picker::PickerDelegate as _;
use crate::quick_open::QuickOpenDelegate;
use crate::test_support;

use super::{AppShell, EditorView, NoteStore, Shown};
use crate::file_kind::FileKind;
use crate::open::{FileActivated, Open};
use crate::sidebar::Sidebar;

mod activation;
mod backlinks;
mod block_scrolls;
mod code_block_wrap;
mod column;
mod command_palette;
mod creating;
mod dock_drag;
mod dock_menu;
mod dock_moves;
mod dock_splits;
mod docks;
mod dragging;
mod drop_marker;
mod edge_drops;
mod failed_trash;
mod files_header;
mod following;
mod geometry;
mod gone;
mod history;
mod history_hold;
mod history_menu;
mod images;
mod modal;
mod nav;
mod notices;
mod open;
mod open_first;
mod pane_keys;
mod parked;
mod quick_open;
mod quitting;
mod rebuild_index;
mod reload;
mod renaming;
mod reopen;
mod restore_lands;
mod restore_refused;
mod separators;
mod session_writes;
mod sidebar_assets;
mod sidebar_selection;
mod split_close;
mod splits;
mod splits_tree;
mod tab_bar;
mod tab_chrome;
mod tab_keys;
mod tab_menu;
mod tab_moves;
mod tabs;
mod themes;
mod title_bar;
mod trashing;
mod vault_search;
mod window_bounds;

impl AppShell {
    pub(crate) fn tree_row_names(&self, cx: &gpui::App) -> Vec<String> {
        self.panes().sidebar.read(cx).row_names()
    }

    pub(crate) fn toggle_tree_row(&mut self, name: &str, cx: &mut gpui::Context<Self>) {
        self.panes()
            .sidebar
            .update(cx, |sidebar, cx| sidebar.toggle_named(name, cx));
    }

    /// The open palette's row labels; `None` when it is not open.
    pub(crate) fn palette_names(&self, cx: &gpui::App) -> Option<Vec<String>> {
        let picker = self.modal_picker::<crate::command_palette::CommandPaletteDelegate>()?;
        let delegate = &picker.read(cx).delegate;
        Some(
            (0..delegate.match_count())
                .map(|ix| delegate.row_label(ix).to_string())
                .collect(),
        )
    }

    /// The open quick-open picker's empty text and match count; `None` when it is not open.
    pub(crate) fn quick_open_state(&self, cx: &gpui::App) -> Option<(Option<String>, usize)> {
        let picker = self.modal_picker::<QuickOpenDelegate>()?;
        let delegate = &picker.read(cx).delegate;
        Some((
            delegate.empty_text(cx).map(String::from),
            delegate.match_count(),
        ))
    }
}

/// Rooted at `AppShell`, not `Sidebar`, so `AppShell::new`'s subscription is reachable.
pub(super) fn open_vault(
    cx: &mut TestAppContext,
    fs: Arc<dyn Fs>,
) -> (WindowHandle<AppShell>, Entity<Sidebar>, Entity<NoteStore>) {
    let notes = test_support::scan_notes(cx, fs);
    let window = cx.add_window({
        let notes = notes.clone();
        move |window, cx| AppShell::new(None, notes, None, None, window, cx)
    });
    let sidebar = window
        .update(cx, |shell, _, _| shell.panes().sidebar.clone())
        .expect("the shell is open");
    (window, sidebar, notes)
}

pub(super) fn showing(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Option<EntryId> {
    window
        .update(cx, |shell, _, _| shell.showing())
        .expect("the shell is open")
}

/// The image the active group shows, if any.
pub(super) fn image_shown(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> Option<EntryId> {
    window
        .update(cx, |shell, _, _| match shell.center().shown {
            Shown::Image(id) => Some(id),
            _ => None,
        })
        .expect("the shell is open")
}

/// Panics if no note has been activated yet — that is the contract.
pub(super) fn editor(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> Entity<EditorView> {
    window
        .update(cx, |shell, _, _| shell.editor().cloned())
        .expect("the shell is open")
        .expect("a note has been activated, so the editor exists")
}

pub(super) fn two_notes(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (
    WindowHandle<AppShell>,
    Entity<Sidebar>,
    Entity<NoteStore>,
    EntryId,
    EntryId,
) {
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let a = test_support::entry_id(&notes, cx, "a");
    let b = test_support::entry_id(&notes, cx, "b");
    (window, sidebar, notes, a, b)
}

pub(super) fn activate(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, id: EntryId) {
    sidebar.update(cx, |_, cx| {
        cx.emit(FileActivated {
            file: id,
            open: Open::Here,
        });
    });
}

/// Must draw the sidebar before focusing it: `track_focus` only registers on render.
pub(super) fn tree_focused(
    cx: &mut TestAppContext,
    window: WindowHandle<AppShell>,
    _sidebar: &Entity<Sidebar>,
) -> VisualTestContext {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            let handle = shell.tree_focus_handle(cx);
            window.focus(&handle, cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    visual
}

/// No vault, keys bound, the shell's root focused and drawn.
pub(super) fn no_vault(cx: &mut TestAppContext) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = cx.update(|cx| {
        gpui::AppContext::new(cx, |cx| NoteStore::new(Arc::new(MemFs::new()), None, cx))
    });
    let window =
        cx.add_window(move |window, cx| AppShell::new(None, notes, None, None, window, cx));
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            window.focus(&gpui::Focusable::focus_handle(shell, cx), cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    (window, visual)
}

pub(super) fn vault_names(notes: &Entity<NoteStore>, cx: &mut TestAppContext) -> Vec<String> {
    notes.update(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
            .collect()
    })
}

/// A vault on `fs`, keys bound, the shell's root focused and drawn.
pub(super) fn shell_focused(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (WindowHandle<AppShell>, Entity<NoteStore>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let (window, _sidebar, notes) = open_vault(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle, cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    (window, notes, visual)
}

/// `a.md` ("alpha") and `b.md` ("beta") behind `shell_focused`.
pub(super) fn ab_focused(
    cx: &mut TestAppContext,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    EntryId,
    EntryId,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (window, notes, mut visual) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut visual, "a");
    let b = test_support::entry_id(&notes, &mut visual, "b");
    (fs, window, visual, a, b)
}

/// Opens `id` into the active tab and lets it land.
pub(super) fn open(window: &WindowHandle<AppShell>, cx: &mut TestAppContext, id: EntryId) {
    window
        .update(cx, |shell, _, cx| {
            shell.open_file(id, FileKind::Note, None, cx);
        })
        .expect("the shell is open");
    cx.run_until_parked();
}

pub(super) fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// Each tab's file, and the active tab's index.
pub(super) fn tab_files(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> (Vec<Option<EntryId>>, usize) {
    window
        .update(cx, |shell, _, _| {
            (
                shell.tab_files(shell.layout.last_center()).collect(),
                shell.center().tabs.active_index(),
            )
        })
        .expect("the shell is open")
}

/// The editor's selection and top line.
pub(super) fn view_of(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> (Range<ByteOffset>, ByteOffset) {
    editor(window, cx).update(cx, |editor, _| (editor.selected(), editor.offset_at_top()))
}

pub(super) fn select(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
    range: Range<usize>,
) {
    window
        .update(cx, |shell, window, cx| {
            let editor = shell.editor().cloned().expect("a note is showing");
            editor.update(cx, |editor, cx| {
                editor.select_range(
                    ByteOffset::new(range.start)..ByteOffset::new(range.end),
                    window,
                    cx,
                );
            });
        })
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn text_of(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
    id: EntryId,
) -> String {
    window
        .update(cx, |shell, _, cx| {
            let notes = shell.notes.read(cx);
            notes.buffer_for(id).unwrap().read(cx).text().to_owned()
        })
        .unwrap()
}

/// H8, T3, no buffer for a `Trashing` note, and an image held only while a group shows it; T2 holds by construction (`Shown`), T1 by H8.
/// I1 and I2 hold by type, an inactive slot holding the `Parked`; `Tabs::close` panics on a slot that breaks H8.
pub(super) fn check(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) {
    window
        .update(cx, |shell, _, cx| {
            let active = shell.center().tabs.active();
            assert!(
                shell
                    .center()
                    .tabs
                    .iter()
                    .all(|tab| tab.place().is_none() == (tab.id() == active)),
                "H8"
            );
            assert_eq!(shell.notes.read(cx).current(), shell.bound_note(), "T3");
            if let Shown::Trashing(id) = shell.center().shown {
                assert!(shell.notes.read(cx).buffer_for(id).is_none(), "Trashing");
            }
            let shown: HashSet<EntryId> = shell
                .layout
                .center_groups()
                .into_iter()
                .filter_map(|group| match shell.group(group).shown {
                    Shown::Image(id) => Some(id),
                    _ => None,
                })
                .collect();
            if let Some(panes) = &shell.panes {
                assert_eq!(panes.images.read(cx).ids(), shown, "images");
            }
        })
        .unwrap();
}

pub(super) fn tall_notes(
    cx: &mut TestAppContext,
) -> (WindowHandle<AppShell>, VisualTestContext, EntryId, EntryId) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "line\n".repeat(200));
    fs.insert_file("/vault/b.md", "line\n".repeat(200));
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    (window, cx, a, b)
}

pub(super) fn scroll_to(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
    offset: usize,
) -> ByteOffset {
    editor(window, cx).update(cx, |editor, cx| {
        editor.scroll_to_top(ByteOffset::new(offset), cx);
    });
    cx.run_until_parked();
    editor(window, cx).update(cx, |editor, _| editor.offset_at_top())
}

pub(super) fn trash(window: &WindowHandle<AppShell>, cx: &mut TestAppContext, id: EntryId) {
    let trashed = start_trash(window, cx, id);
    assert!(futures::executor::block_on(trashed), "the trash succeeded");
}

/// Starts trashing `id` without letting it finish: it stays `Trashing` while its chain is held.
pub(super) fn start_trash(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
    id: EntryId,
) -> Task<bool> {
    let notes = window
        .update(cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    let trashing = notes.update(cx, |notes, cx| notes.trash(id, cx));
    cx.run_until_parked();
    trashing
}

/// `a` was typed into and left with its save held at `gate`, so the trash waits `Trashing` behind it.
pub(super) fn finish_trash(
    fs: &MemFs,
    gate: Gate,
    trashing: Task<bool>,
    cx: &mut VisualTestContext,
) {
    gate.release();
    cx.run_until_parked();
    assert!(futures::executor::block_on(trashing), "the trash succeeded");
    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("alphax")
    );
}

/// Lets a trash held behind `gate` fail: its note was made untrashable, so it stays open.
pub(super) fn fail_trash(gate: Gate, trashing: Task<bool>, cx: &mut VisualTestContext) {
    gate.release();
    cx.run_until_parked();
    assert!(!futures::executor::block_on(trashing), "the trash failed");
}

/// `a`–`d` ("alpha", "beta", "gamma", "delta") behind `shell_focused`.
pub(super) fn four_notes(
    cx: &mut TestAppContext,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    [EntryId; 4],
) {
    let fs = Arc::new(MemFs::new());
    for (name, text) in [
        ("a", "alpha"),
        ("b", "beta"),
        ("c", "gamma"),
        ("d", "delta"),
    ] {
        fs.insert_file(format!("/vault/{name}.md"), text);
    }
    let (window, notes, mut visual) = shell_focused(cx, &fs);
    let ids = ["a", "b", "c", "d"].map(|name| test_support::entry_id(&notes, &mut visual, name));
    (fs, window, visual, ids)
}

/// Where the shell's tree drew `row`, read from the window's root.
pub(super) fn row_point(cx: &mut VisualTestContext, row: f32) -> Point<Pixels> {
    cx.update(|window, cx| {
        let shell = window
            .root::<AppShell>()
            .flatten()
            .expect("the shell is the root");
        shell.read(cx).tree_row_center(row, cx)
    })
    .expect("the tree is drawn")
}

/// Rows 0–3 are `a`–`d` in `four_notes`.
pub(super) fn click_row(cx: &mut VisualTestContext, row: f32, modifiers: Modifiers) {
    let at = row_point(cx, row);
    cx.simulate_click(at, modifiers);
    cx.run_until_parked();
}

pub(super) fn editor_focused(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    let editor = editor(window, cx);
    window
        .update(cx, |_, window, cx| {
            gpui::Focusable::focus_handle(&editor, cx).is_focused(window)
        })
        .unwrap()
}

pub(super) fn focus_tree(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.tree_focus_handle(cx), cx);
        })
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

/// Starts `id`'s load into the active tab and leaves it pending behind its gate.
pub(super) fn start_open(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, id: EntryId) {
    window
        .update(cx, |shell, _, cx| {
            shell.open_file(id, FileKind::Note, None, cx);
        })
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn selected(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Option<EntryId> {
    window
        .update(cx, |shell, _, cx| shell.panes().sidebar.read(cx).selected())
        .unwrap()
}
