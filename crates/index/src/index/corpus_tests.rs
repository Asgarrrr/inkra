use inkra_vault::Stamp;

use super::NoteIndex;
use crate::test_support::{ROOT, content, id, load, loaded, mtime, note_set, rename, vault};

/// The stamp `load` gives a text of `size` bytes.
fn at0(size: u64) -> Stamp {
    Stamp {
        mtime: mtime(0),
        size,
    }
}

#[test]
fn insert_stores_the_stamp_and_remove_clears_it() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    let empty = index.corpus().heap_bytes();

    load(&mut index, b, "bee");
    load(&mut index, a, "[[b]]");
    assert_eq!(loaded(&index.snapshot()), [(a, at0(5)), (b, at0(3))]);
    assert_eq!(index.corpus().heap_bytes(), empty, "a slot per note");

    load(&mut index, a, "x");
    index.remove(b);
    assert_eq!(loaded(&index.snapshot()), [(a, at0(1))]);
    index.remove(b);
    assert_eq!(
        loaded(&index.snapshot()),
        [(a, at0(1))],
        "a second remove changes nothing"
    );
    assert_eq!(index.corpus().heap_bytes(), empty);
}

#[test]
fn a_text_for_a_non_note_is_ignored() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, inkra_vault::EntryId::DETACHED, "x");
    assert_eq!(loaded(&index.snapshot()), []);
}

#[test]
fn set_notes_keeps_each_note_s_stamp_across_a_create_rename_and_trash() {
    let (fs, mut vault) = vault(&["b.md", "d.md", "gone.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (b, d, gone) = (
        id(&vault, "b.md"),
        id(&vault, "d.md"),
        id(&vault, "gone.md"),
    );
    load(&mut index, b, "bee");
    load(&mut index, d, "deee");
    load(&mut index, gone, "gone");

    fs.insert_file(format!("{ROOT}/a.md"), "");
    fs.remove(std::path::Path::new(&format!("{ROOT}/gone.md")));
    rename(&fs, &mut vault, "d.md", "c.md");
    let held = index.snapshot();
    let _ = index.set_notes(note_set(&vault));

    assert_eq!(id(&vault, "c.md"), d, "a rename keeps the id");
    assert_eq!(loaded(&index.snapshot()), [(b, at0(3)), (d, at0(4))]);
    assert_eq!(index.corpus().len(), 3, "a, b, c");
    assert_eq!(loaded(&held).len(), 3, "the held snapshot is untouched");
}

#[test]
fn a_held_snapshot_never_sees_a_later_insert() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    index.insert(a, content("old", mtime(1)));
    let held = index.snapshot();
    index.insert(a, content("new", mtime(2)));
    let at = |n| Stamp {
        mtime: mtime(n),
        size: 3,
    };
    assert_eq!(loaded(&held), [(a, at(1))]);
    assert_eq!(loaded(&index.snapshot()), [(a, at(2))]);
}

#[test]
fn a_save_moves_the_note_s_mtime_without_a_rescan() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    let scanned = |id| vault.entry(id).map(|entry| entry.mtime);
    assert_eq!(index.corpus().modified(1), scanned(b));

    index.insert(a, content("loaded", mtime(1)));
    index.insert(a, content("saved", mtime(2)));

    let corpus = index.corpus();
    assert_eq!(corpus.modified(0), Some(mtime(2)));
    assert_eq!(
        corpus.modified(1),
        scanned(b),
        "a note not written keeps its scan's"
    );
    let saved = Stamp {
        mtime: mtime(2),
        size: 5,
    };
    assert_eq!(loaded(&index.snapshot()), [(a, saved)]);
}
