use std::sync::atomic::AtomicBool;

use inkra_vault::EntryId;

use super::NoteSet;
use crate::test_support::{entries, vault};
use crate::{NameQuery, match_names};

/// Every candidate's path, in MRU order, which `entries()` order gives.
fn paths(files: &[&str]) -> Vec<String> {
    let (_fs, vault) = vault(files);
    let all = entries(&vault);
    let recent: Vec<EntryId> = all.iter().map(|(id, _, _)| *id).collect();
    let names = NoteSet::build(all).names();
    let query = NameQuery::new("", None, &recent);
    match_names(
        &names,
        0..names.len(),
        &query,
        &AtomicBool::new(false),
        usize::MAX,
    )
    .into_iter()
    .map(|found| found.path.to_string())
    .collect()
}

#[test]
fn notes_are_candidates_in_vault_order_and_folders_are_not() {
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
    let names = NoteSet::build(entries(&vault)).names();
    assert_eq!(names.len(), 10);
    assert!(!names.is_empty());
    assert_eq!(names.chunks(3), [0..3, 3..6, 6..10]);
    assert!(NoteSet::build(Vec::new()).names().chunks(4).is_empty());
    assert!(NoteSet::build(Vec::new()).names().is_empty());
}
