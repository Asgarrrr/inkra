//! Fixtures shared by window-free tests.
//!
//! Scanning a `MemFs` vault into a `NoteStore` entity, naming an entry by its
//! display name, and the two-note starting point several suites build on all
//! existed in near-duplicate form across `shell.rs`, `vault/tests/mod.rs` and
//! `sidebar/tests.rs`. What is here is only the part none of them needs a
//! window for.
//!
//! A helper that roots a window stays beside the view it exercises instead,
//! per `CLAUDE.md`'s testing rule: `with_editor` / `dispatching`
//! (`editor/tests/mod.rs`), `open_vault` / `tree_focused`
//! (`shell/tests/mod.rs`), `drawn` (`sidebar/tests.rs`), `open_shell`
//! (`vault/tests/mod.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, prelude::*};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use crate::vault::NoteStore;

/// Scans `fs` at `/vault` and wraps the result behind `Entity<Vault>` and
/// `Entity<NoteStore>` — the shape every window-free vault test starts from.
pub(crate) fn scan_notes(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
    let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
        .expect("scan the test vault");
    let vault = cx.update(|cx| cx.new(|_| scanned));
    cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)))
}

/// By name, never by index: the comparator decides the order.
pub(crate) fn entry_id(notes: &Entity<NoteStore>, cx: &mut TestAppContext, name: &str) -> EntryId {
    notes.update(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .find(|entry| entry.path.display_name() == name)
            .map(|entry| entry.id)
            .expect("the entry is in the vault")
    })
}

/// `a.md` / `b.md`, scanned and identified. The fixture most window-free
/// suites build on. A suite that needs a window wraps this rather than
/// reinserting the same two files — `shell/tests::two_notes` does, on top of
/// `open_vault`.
pub(crate) fn two_notes(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (Entity<NoteStore>, EntryId, EntryId) {
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    let b = entry_id(&notes, cx, "b");
    (notes, a, b)
}
