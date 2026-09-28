#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::keymap::key_bindings;
use crate::picker::{Picker, PickerDelegate as _};
use crate::quick_open::QuickOpenDelegate;
use crate::test_support;

use super::{AppShell, EditorView, NoteActivated, NoteStore, Sidebar};

mod activation;
mod backlinks;
mod column;
mod creating;
mod following;
mod gone;
mod modal;
mod open_first;
mod quick_open;
mod reload;
mod renaming;
mod trashing;

impl AppShell {
    pub(crate) fn tree_row_names(&self, cx: &gpui::App) -> Vec<String> {
        self.sidebar.read(cx).row_names()
    }

    pub(crate) fn toggle_tree_row(&mut self, name: &str, cx: &mut gpui::Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.toggle_named(name, cx));
    }

    /// The open quick-open picker's empty text and match count; `None` when it is not open.
    pub(crate) fn quick_open_state(&self, cx: &gpui::App) -> Option<(Option<&'static str>, usize)> {
        let picker = self
            .modal
            .as_ref()?
            .view()
            .clone()
            .downcast::<Picker<QuickOpenDelegate>>()
            .ok()?;
        let delegate = &picker.read(cx).delegate;
        Some((delegate.empty_text(), delegate.match_count()))
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
