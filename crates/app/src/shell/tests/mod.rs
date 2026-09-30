#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::keymap::key_bindings;
use crate::picker::PickerDelegate as _;
use crate::quick_open::QuickOpenDelegate;
use crate::test_support;

use super::{AppShell, EditorView, NoteActivated, NoteStore, Sidebar};

mod activation;
mod backlinks;
mod column;
mod command_palette;
mod creating;
mod following;
mod gone;
mod modal;
mod open_first;
mod quick_open;
mod reload;
mod renaming;
mod tab_keys;
mod tabs;
mod trashing;
mod vault_search;

impl AppShell {
    pub(crate) fn tree_row_names(&self, cx: &gpui::App) -> Vec<String> {
        self.sidebar.read(cx).row_names()
    }

    pub(crate) fn toggle_tree_row(&mut self, name: &str, cx: &mut gpui::Context<Self>) {
        self.sidebar
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
        move |_, cx| AppShell::new(None, notes, cx)
    });
    let sidebar = window
        .update(cx, |shell, _, _| shell.sidebar.clone())
        .expect("the shell is open");
    (window, sidebar, notes)
}

pub(super) fn showing(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Option<EntryId> {
    window
        .update(cx, |shell, _, _| shell.showing)
        .expect("the shell is open")
}

/// Panics if no note has been activated yet — that is the contract.
pub(super) fn editor(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> Entity<EditorView> {
    window
        .update(cx, |shell, _, _| shell.editor.clone())
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
    sidebar.update(cx, |_, cx| cx.emit(NoteActivated(id)));
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
    let window = cx.add_window(move |_, cx| AppShell::new(None, notes, cx));
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
        .update(cx, |shell, _, cx| shell.open_note(id, None, cx))
        .expect("the shell is open");
    cx.run_until_parked();
}

/// Each tab's note, and the active tab's index.
pub(super) fn tab_notes(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> (Vec<Option<EntryId>>, usize) {
    window
        .update(cx, |shell, _, _| {
            let notes = shell.tabs.iter().map(|tab| tab.place().note).collect();
            (notes, shell.tabs.active_index())
        })
        .expect("the shell is open")
}
