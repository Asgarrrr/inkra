#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, prelude::*};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use crate::vault::NoteStore;

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
