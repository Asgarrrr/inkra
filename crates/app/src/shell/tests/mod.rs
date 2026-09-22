//! `AppShell` end to end: renaming (`renaming.rs`), creating (`creating.rs`),
//! trashing (`trashing.rs`), and which note the editor ends up showing
//! (`activation.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::keymap::key_bindings;
use crate::test_support;

use super::{AppShell, EditorView, NoteActivated, NoteStore, Sidebar};

mod activation;
mod creating;
mod renaming;
mod trashing;

/// Rooted at `AppShell`, not at the `Sidebar`: that puts the sidebar under
/// its real `SIDEBAR_WIDTH` parent, and it is the only way to reach the subscription
/// `AppShell::new` sets up.
/// No editor: a vault launch has none until a row is activated, which is the
/// state this file exists to exercise. Reach it with [`editor`] afterwards.
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

/// The editor, once a note has been activated.
///
/// Panics before that, and that is the contract: under an optional editor,
/// "there is an editor" is something a test establishes rather than
/// something it is handed. Reads the private field directly, as `open_vault`
/// already does for the sidebar — this module is a descendant of `shell`.
pub(super) fn editor(
    window: &WindowHandle<AppShell>,
    cx: &mut TestAppContext,
) -> Entity<EditorView> {
    window
        .update(cx, |shell, _, _| shell.editor.clone())
        .expect("the shell is open")
        .expect("a note has been activated, so the editor exists")
}

/// Returns the window rather than an editor: there is none yet, and its
/// callers need the handle to ask [`editor`] for one after activating.
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

/// Indistinguishable from a click to `AppShell`, and it needs no `Row`, so
/// nothing in `sidebar.rs` widens for it.
pub(super) fn activate(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, id: EntryId) {
    sidebar.update(cx, |_, cx| cx.emit(NoteActivated(id)));
}

/// A real window with the bindings installed and the sidebar focused.
///
/// The sidebar has to be *drawn* before it can be focused: `track_focus`
/// only puts the handle in a dispatch node when the element renders.
pub(super) fn tree_focused(
    cx: &mut TestAppContext,
    window: WindowHandle<AppShell>,
    sidebar: &Entity<Sidebar>,
) -> VisualTestContext {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            let handle = sidebar.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    visual
}

/// Names the vault holds now, in tree order.
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
