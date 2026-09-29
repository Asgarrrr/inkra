#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use futures::executor::block_on;
use gpui::{Entity, Subscription, TestAppContext, prelude::*};
use inkra_index::{Group, NoteIndex, NoteMatch, NoteQuery};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Serial, Vault};

use crate::index::IndexSync;
use crate::vault::{ConflictPreserved, NoteStore};

/// A vault of empty notes, `files` relative to `/vault`.
pub(crate) fn vault(files: &[&str]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for file in files {
        fs.insert_file(format!("/vault/{file}"), "");
    }
    fs
}

pub(crate) fn scan_notes(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
    let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
        .expect("scan the test vault");
    let vault = cx.update(|cx| cx.new(|_| scanned));
    cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)))
}

/// A store over a vault nothing has read yet: it scans the root, then the rest, as a launch does.
pub(crate) fn unscanned_notes(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
    cx.update(|cx| {
        let vault = cx.new(|_| Vault::new(Path::new("/vault")));
        cx.new(|cx| NoteStore::new(fs, Some(vault), cx))
    })
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

/// Opens the note and makes it current.
pub(crate) async fn opened(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
    id: EntryId,
) -> Entity<Buffer> {
    let buffer = notes
        .update(cx, |notes, cx| notes.open_note(id, cx))
        .await
        .expect("the note opens");
    notes.update(cx, |notes, cx| notes.set_current(id, cx));
    buffer
}

/// The paths of every `ConflictPreserved` the store emits.
pub(crate) fn conflicts(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<PathBuf>>>, Subscription) {
    let preserved = Rc::new(RefCell::new(Vec::new()));
    let subscription = cx.update(|cx| {
        let preserved = Rc::clone(&preserved);
        cx.subscribe(notes, move |_, event: &ConflictPreserved, _| {
            preserved.borrow_mut().push(event.path.clone());
        })
    });
    (preserved, subscription)
}

/// Inserts `text` at the start; only the notify fires the autosave observer.
pub(crate) fn edit(buffer: &Entity<Buffer>, cx: &mut TestAppContext, text: &str) {
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), text));
        cx.notify();
    });
}

/// The ids quick open lists for `query`.
pub(crate) fn named(
    links: &Entity<IndexSync>,
    cx: &mut TestAppContext,
    query: &str,
) -> Vec<EntryId> {
    links.read_with(cx, |links, _| {
        let index = links.index();
        let query = NoteQuery::new(query, None, &[]);
        quick_open(index, &query, 10)
            .into_iter()
            .filter(|found| found.group == Group::Name)
            .map(|found| found.id)
            .collect()
    })
}

/// Quick open's matches for `query`, best first.
pub(crate) fn quick_open(index: &NoteIndex, query: &NoteQuery, limit: usize) -> Vec<NoteMatch> {
    let snapshot = index.snapshot();
    block_on(snapshot.find_notes(query, limit, &Serial, &AtomicBool::new(false)))
        .expect("not cancelled")
}
