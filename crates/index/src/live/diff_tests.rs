//! The snapshot comparison a `BuildJob` runs off the foreground (coalesced-reconcile spec, R2).

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Entry, EntryId, Mtime, RelPath, Stamp, Vault};
use proptest::prelude::*;

use super::diff::{Change, notes};
use crate::find::{find_in, merge_found};
use crate::{NoteIndex, NoteMatch, NoteQuery};

/// Notes in two folders beside an empty one, as a scan lists them.
fn snapshot() -> Vec<Entry> {
    let fs = MemFs::new();
    for file in ["a.md", "b.md", "f/c.md", "f/d.md", "g/e.md"] {
        fs.insert_file(format!("/vault/{file}"), "");
    }
    fs.insert_dir("/vault/empty");
    let vault = block_on(Vault::scan(&fs, Path::new("/vault"))).expect("a MemFs scan");
    vault.entries().cloned().collect()
}

/// `None` for a `Layout`.
fn stamps(last: Option<&[Entry]>, entries: &[Entry]) -> Option<Vec<(EntryId, Stamp)>> {
    match Change::between(last, entries) {
        Change::Stamps(moved) => Some(moved),
        Change::Layout(_) => None,
    }
}

fn mtime(secs: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn position(entries: &[Entry], path: &str) -> usize {
    entries
        .iter()
        .position(|entry| entry.path.as_path() == Path::new(path))
        .expect("the entry is in the snapshot")
}

#[test]
fn no_last_snapshot_is_a_layout() {
    assert_eq!(stamps(None, &snapshot()), None);
    assert_eq!(
        stamps(None, &[]),
        None,
        "a vault with no notes still builds once"
    );
}

#[test]
fn the_same_snapshot_moves_no_stamp() {
    let last = snapshot();
    assert_eq!(stamps(Some(&last), &last), Some(Vec::new()));
}

#[test]
fn one_mtime_moved_is_that_note() {
    let last = snapshot();
    let mut entries = last.clone();
    let c = position(&entries, "f/c.md");
    entries[c].mtime = mtime(7);
    assert_eq!(
        stamps(Some(&last), &entries),
        Some(vec![(entries[c].id, entries[c].stamp())])
    );
}

#[test]
fn a_size_only_change_is_that_note() {
    let last = snapshot();
    let mut entries = last.clone();
    let c = position(&entries, "f/c.md");
    entries[c].size = 9;
    assert_eq!(
        stamps(Some(&last), &entries),
        Some(vec![(entries[c].id, entries[c].stamp())])
    );
}

#[test]
fn a_folder_added_or_removed_moves_no_stamp() {
    let last = snapshot();
    let mut entries = last.clone();
    entries.remove(position(&entries, "empty"));
    assert_eq!(stamps(Some(&last), &entries), Some(Vec::new()), "removed");
    assert_eq!(stamps(Some(&entries), &last), Some(Vec::new()), "added");
}

#[test]
fn a_note_path_id_count_or_order_change_is_a_layout() {
    let last = snapshot();
    let (a, b) = (position(&last, "a.md"), position(&last, "b.md"));

    let mut moved = last.clone();
    moved[a].path = RelPath::new(Path::new("z.md")).expect("a relative path");
    assert_eq!(stamps(Some(&last), &moved), None, "path");

    let mut rekeyed = last.clone();
    rekeyed[a].id = EntryId::DETACHED;
    assert_eq!(stamps(Some(&last), &rekeyed), None, "id");

    let mut fewer = last.clone();
    fewer.remove(b);
    assert_eq!(stamps(Some(&last), &fewer), None, "count");
    assert_eq!(stamps(Some(&fewer), &last), None, "count");

    let mut swapped = last.clone();
    swapped.swap(a, b);
    assert_eq!(stamps(Some(&last), &swapped), None, "order");
}

#[derive(Debug, Clone)]
enum Edit {
    Mtime(usize, u64),
    Size(usize, u64),
    Path(usize, usize),
    Id(usize, usize),
    Remove(usize),
    Duplicate(usize),
    Swap(usize, usize),
}

fn edit(entries: usize) -> impl Strategy<Value = Edit> {
    prop_oneof![
        4 => (0..entries, 0..3u64).prop_map(|(at, secs)| Edit::Mtime(at, secs)),
        2 => (0..entries, 0..3u64).prop_map(|(at, size)| Edit::Size(at, size)),
        1 => (0..entries, 0..entries).prop_map(|(at, from)| Edit::Path(at, from)),
        1 => (0..entries, 0..entries).prop_map(|(at, from)| Edit::Id(at, from)),
        1 => (0..entries).prop_map(Edit::Remove),
        1 => (0..entries).prop_map(Edit::Duplicate),
        1 => (0..entries, 0..entries).prop_map(|(at, with)| Edit::Swap(at, with)),
    ]
}

/// Indices wrap, since an edit may shorten the snapshot the next one indexes.
fn apply(entries: &mut Vec<Entry>, edit: &Edit) {
    let len = entries.len();
    if len == 0 {
        return;
    }
    match *edit {
        Edit::Mtime(at, secs) => entries[at % len].mtime = mtime(secs),
        Edit::Size(at, size) => entries[at % len].size = size,
        Edit::Path(at, from) => entries[at % len].path = entries[from % len].path.clone(),
        Edit::Id(at, from) => entries[at % len].id = entries[from % len].id,
        Edit::Remove(at) => {
            entries.remove(at % len);
        }
        Edit::Duplicate(at) => entries.insert(at % len, entries[at % len].clone()),
        Edit::Swap(at, with) => entries.swap(at % len, with % len),
    }
}

/// The snapshot before and after `edits`.
fn edited(edits: &[Edit]) -> (Vec<Entry>, Vec<Entry>) {
    let last = snapshot();
    let mut entries = last.clone();
    for edit in edits {
        apply(&mut entries, edit);
    }
    (last, entries)
}

fn quick_open(index: &NoteIndex, query: &NoteQuery, at: usize) -> Vec<NoteMatch> {
    let snapshot = index.snapshot();
    let cancel = AtomicBool::new(false);
    let lookup = snapshot.lookup(query, &cancel).expect("not cancelled");
    let kept = find_in(&snapshot, &lookup, at..at + 1, query, &cancel, 1);
    merge_found(&snapshot, vec![kept], query, 1)
}

/// The `(id, path)` at each position of the names a build of `entries` makes.
fn names(entries: &[Entry]) -> Vec<(EntryId, String)> {
    let Change::Layout(built) = Change::between(None, entries) else {
        panic!("with no last snapshot, a build");
    };
    let mut index = NoteIndex::default();
    let _ = index.set_notes(built.notes);
    let every: Vec<EntryId> = entries.iter().map(|entry| entry.id).collect();
    // An empty query lists exactly its recent notes, so one position at a time reads the order.
    let query = NoteQuery::new("", None, &every);
    (0..index.names().len())
        .flat_map(|at| quick_open(&index, &query, at))
        .map(|found| (found.id, found.path.to_string()))
        .collect()
}

proptest! {
    /// `Stamps` exactly when the notes' ids and paths are equal position by position, holding
    /// the notes whose stamp differs.
    #[test]
    fn stamps_exactly_when_the_notes_ids_and_paths_match(
        edits in prop::collection::vec(edit(snapshot().len()), 0..4),
    ) {
        let (last, entries) = edited(&edits);

        let (before, after): (Vec<_>, Vec<_>) = (notes(&last).collect(), notes(&entries).collect());
        let same = before.len() == after.len()
            && before
                .iter()
                .zip(&after)
                .all(|(old, new)| old.id == new.id && old.path == new.path);
        let expected = same.then(|| {
            before
                .iter()
                .zip(&after)
                .filter(|(old, new)| old.stamp() != new.stamp())
                .map(|(_, new)| (new.id, new.stamp()))
                .collect()
        });
        prop_assert_eq!(stamps(Some(&last), &entries), expected);
    }

    /// Whenever the diff skips the build, a build would have made the same names.
    #[test]
    fn stamps_only_when_a_build_would_make_the_same_names(
        edits in prop::collection::vec(edit(snapshot().len()), 0..4),
    ) {
        let (last, entries) = edited(&edits);

        if stamps(Some(&last), &entries).is_some() {
            prop_assert_eq!(names(&last), names(&entries));
        }
    }
}
