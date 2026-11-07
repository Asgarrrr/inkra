use super::NoteSet;
use crate::test_support::{entries, vault};

/// Every name's path, by position.
fn paths(files: &[&str]) -> Vec<String> {
    let (_fs, vault) = vault(files);
    let names = NoteSet::build(entries(&vault)).names;
    (0..names.len())
        .map(|order| names.path(order).to_string())
        .collect()
}

#[test]
fn names_are_in_vault_order_and_folders_are_not() {
    assert_eq!(paths(&["b.md", "a/c.md"]), ["a/c", "b"]);
}

#[test]
fn the_md_extension_is_stripped_in_any_case() {
    assert_eq!(paths(&["Plan.MD"]), ["Plan"]);
}

#[test]
fn paths_are_stored_nfc() {
    assert_eq!(paths(&["e\u{301}te\u{301}.md"]), ["été"]);
}

#[test]
fn a_snapshot_splits_into_ranges_over_its_notes() {
    let files: Vec<String> = (0..10).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let names = NoteSet::build(entries(&vault)).names;
    assert_eq!(names.len(), 10);
    assert!(!names.is_empty());
    assert_eq!(names.chunks(3), [0..3, 3..6, 6..10]);
    let empty = NoteSet::build(Vec::<(_, String, _, _)>::new()).names;
    assert!(empty.chunks(4).is_empty());
    assert!(empty.is_empty());
}

#[test]
fn a_name_position_is_its_note_order() {
    let (_fs, vault) = vault(&["b.md", "a/c.md", "a/d.md", "e.md"]);
    let set = NoteSet::build(entries(&vault));
    assert_eq!(set.names.len(), 4);
    for at in 0..set.names.len() {
        assert_eq!(set.notes.order(set.names.id(at)), at, "name {at}");
    }
}

#[test]
fn a_note_s_file_is_its_path_as_scanned() {
    let (_fs, vault) = vault(&["Notes/Cafe\u{301}.MD"]);
    let names = NoteSet::build(entries(&vault)).names;
    assert_eq!(&**names.file(0), "Notes/Cafe\u{301}.MD");
    assert_eq!(&**names.path(0), "Notes/Café");
}
