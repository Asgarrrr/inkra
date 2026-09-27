use std::sync::Arc;

use super::NoteIndex;
use crate::NoteContent;
use crate::corpus::TEXT_HEADER_BYTES;
use crate::test_support::{ROOT, id, load, mtime, note_set, rename, texts, vault};

#[test]
fn insert_stores_the_text_and_remove_clears_it() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    let empty = index.corpus().heap_bytes();

    load(&mut index, b, "bee");
    load(&mut index, a, "[[b]]");
    assert_eq!(
        texts(&index.corpus()),
        [(a, "[[b]]".to_owned()), (b, "bee".to_owned())]
    );
    assert_eq!(
        index.corpus().heap_bytes(),
        empty + 2 * TEXT_HEADER_BYTES + 8
    );

    load(&mut index, a, "x");
    index.remove(b);
    assert_eq!(texts(&index.corpus()), [(a, "x".to_owned())]);
    assert_eq!(index.corpus().heap_bytes(), empty + TEXT_HEADER_BYTES + 1);
    index.remove(b);
    assert_eq!(
        index.corpus().heap_bytes(),
        empty + TEXT_HEADER_BYTES + 1,
        "a second remove changes nothing"
    );
}

#[test]
fn a_text_for_a_non_note_is_ignored() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, inkra_vault::EntryId::DETACHED, "x");
    assert_eq!(texts(&index.corpus()), []);
}

#[test]
fn set_notes_keeps_each_note_s_text_across_a_create_rename_and_trash() {
    let (fs, mut vault) = vault(&["b.md", "d.md", "gone.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (b, d, gone) = (
        id(&vault, "b.md"),
        id(&vault, "d.md"),
        id(&vault, "gone.md"),
    );
    load(&mut index, b, "bee");
    load(&mut index, d, "dee");
    load(&mut index, gone, "gone");

    fs.insert_file(format!("{ROOT}/a.md"), "");
    fs.remove(std::path::Path::new(&format!("{ROOT}/gone.md")));
    rename(&fs, &mut vault, "d.md", "c.md");
    let held = index.corpus();
    let _ = index.set_notes(note_set(&vault));

    assert_eq!(id(&vault, "c.md"), d, "a rename keeps the id");
    assert_eq!(
        texts(&index.corpus()),
        [(b, "bee".to_owned()), (d, "dee".to_owned())]
    );
    assert_eq!(index.corpus().len(), 3, "a, b, c");
    assert_eq!(texts(&held).len(), 3, "the held snapshot is untouched");
}

#[test]
fn a_held_snapshot_never_sees_a_later_insert() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    load(&mut index, a, "old");
    let held = index.corpus();
    load(&mut index, a, "new");
    assert_eq!(texts(&held), [(a, "old".to_owned())]);
    assert_eq!(texts(&index.corpus()), [(a, "new".to_owned())]);
}

#[test]
fn a_gone_note_s_text_is_freed_with_the_previous_set() {
    let (fs, mut vault) = vault(&["a.md", "gone.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let text: Arc<str> = "gone".into();
    let weak = Arc::downgrade(&text);
    index.insert(id(&vault, "gone.md"), NoteContent::parse(text, mtime(0)));

    fs.remove(std::path::Path::new(&format!("{ROOT}/gone.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("rescan");
    };
    let previous = index.set_notes(note_set(&vault));
    assert!(weak.upgrade().is_some(), "the previous set holds the text");
    drop(previous);
    assert!(weak.upgrade().is_none());
}

#[test]
fn a_save_moves_the_note_s_mtime_without_a_rescan() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    let scanned = |id| vault.entry(id).map(|entry| entry.mtime);
    assert_eq!(index.corpus().modified(1), scanned(b));

    index.insert(a, NoteContent::parse("loaded".into(), mtime(1)));
    index.insert(a, NoteContent::parse("saved".into(), mtime(2)));

    let corpus = index.corpus();
    assert_eq!(corpus.modified(0), Some(mtime(2)));
    assert_eq!(
        corpus.modified(1),
        scanned(b),
        "a note not written keeps its scan's"
    );
    assert_eq!(texts(&corpus), [(a, "saved".to_owned())]);
}
