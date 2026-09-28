use std::sync::atomic::AtomicBool;

use inkra_markdown::links;

use super::NoteIndex;
use crate::test_support::{entries, id, load, note_set, rename, texts, vault};
use crate::{Group, NoteQuery, Resolution, find_notes, merge_notes};

/// Each backlink as (source path, the linked text of each occurrence).
fn backlinks(
    index: &NoteIndex,
    vault: &inkra_vault::Vault,
    note: &str,
) -> Vec<(String, Vec<String>)> {
    index
        .backlinks(id(vault, note))
        .into_iter()
        .map(|backlink| {
            let source = vault
                .entries()
                .find(|entry| entry.id == backlink.source)
                .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
                .unwrap_or_default();
            let lits = backlink
                .links
                .iter()
                .map(|link| link.context[link.context_link.clone()].to_owned())
                .collect();
            (source, lits)
        })
        .collect()
}

fn pair(source: &str, lits: &[&str]) -> (String, Vec<String>) {
    (
        source.to_owned(),
        lits.iter().map(|lit| (*lit).to_owned()).collect(),
    )
}

#[test]
fn backlinks_group_occurrences_by_source_in_vault_order() {
    let (_fs, vault) = vault(&["a.md", "b.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(
        &mut index,
        id(&vault, "b.md"),
        "[[Plan]] and [[plan|again]]",
    );
    load(&mut index, id(&vault, "a.md"), "[x](Plan.md) [[Other]]");

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [
            pair("a.md", &["[x](Plan.md)"]),
            pair("b.md", &["[[Plan]]", "[[plan|again]]"])
        ]
    );
}

#[test]
fn a_note_linking_to_itself_is_its_own_backlink() {
    let (_fs, vault) = vault(&["Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "Plan.md"), "[[Plan]]");

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [pair("Plan.md", &["[[Plan]]"])]
    );
}

#[test]
fn a_link_resolving_elsewhere_is_not_a_backlink() {
    let (_fs, vault) = vault(&["src.md", "Plan.md", "work/Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "src.md"), "[[work/Plan]]");

    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert_eq!(
        backlinks(&index, &vault, "work/Plan.md"),
        [pair("src.md", &["[[work/Plan]]"])]
    );
}

#[test]
fn replacing_a_source_with_fewer_links_leaves_no_stale_row() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let src = id(&vault, "src.md");
    load(&mut index, src, "[[x]] [[y]] [[Plan]] [[Plan]]");
    assert!(index.is_indexed(src));
    load(&mut index, src, "[[Plan]]");

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [pair("src.md", &["[[Plan]]"])]
    );
    load(&mut index, src, "nothing");
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert!(index.by_stem.is_empty());
}

#[test]
fn removing_a_source_drops_its_backlinks() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let src = id(&vault, "src.md");
    load(&mut index, src, "[[Plan]]");
    index.remove(src);

    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert!(!index.is_indexed(src));
    assert!(index.by_stem.is_empty());
}

#[test]
fn a_rename_moves_backlinks_without_reinserting() {
    let (fs, mut vault) = vault(&["src.md", "Plan.md", "Draft.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let draft = id(&vault, "Draft.md");
    load(&mut index, id(&vault, "src.md"), "[[Plan]] [[Final]]");

    rename(&fs, &mut vault, "Plan.md", "Old.md");
    rename(&fs, &mut vault, "Draft.md", "Final.md");
    let _ = index.set_notes(note_set(&vault));

    assert_eq!(backlinks(&index, &vault, "Old.md"), []);
    assert_eq!(id(&vault, "Final.md"), draft);
    assert_eq!(
        backlinks(&index, &vault, "Final.md"),
        [pair("src.md", &["[[Final]]"])]
    );
}

#[test]
fn a_note_gone_from_the_vault_loses_its_source() {
    let (fs, mut vault) = vault(&["src.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let src = id(&vault, "src.md");
    load(&mut index, src, "[[Plan]]");

    fs.remove(std::path::Path::new(&format!(
        "{}/src.md",
        crate::test_support::ROOT
    )));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("rescan");
    };
    let _ = index.set_notes(note_set(&vault));

    assert!(!index.is_indexed(src));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
}

#[test]
fn pruning_many_notes_keeps_the_rest() {
    let (fs, mut vault) = vault(&[
        "index.md",
        "gone/a.md",
        "gone/b.md",
        "gone/c.md",
        "keep/a.md",
        "keep/b.md",
        "keep/c.md",
    ]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let sources = [
        ("gone/a.md", "[[index]] [[gone/b]] [[keep/a]]"),
        ("gone/b.md", "[[index]] [[gone/c]] [[keep/b]]"),
        ("gone/c.md", "[[index]] [[gone/a]] [[keep/c]] [[orphan]]"),
        ("keep/a.md", "[[index]] [[keep/b]]"),
        ("keep/b.md", "[[index]] [[keep/c]]"),
        ("keep/c.md", "[[index]] [[keep/a]]"),
    ];
    for (path, text) in sources {
        load(&mut index, id(&vault, path), text);
    }
    let gone: Vec<_> = ["gone/a.md", "gone/b.md", "gone/c.md"]
        .map(|path| id(&vault, path))
        .into();

    for path in ["gone/a.md", "gone/b.md", "gone/c.md"] {
        fs.remove(std::path::Path::new(&format!(
            "{}/{path}",
            crate::test_support::ROOT
        )));
    }
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("rescan");
    };
    let _ = index.set_notes(note_set(&vault));

    assert_eq!(
        backlinks(&index, &vault, "index.md"),
        [
            pair("keep/a.md", &["[[index]]"]),
            pair("keep/b.md", &["[[index]]"]),
            pair("keep/c.md", &["[[index]]"]),
        ]
    );
    assert_eq!(
        backlinks(&index, &vault, "keep/a.md"),
        [pair("keep/c.md", &["[[keep/a]]"])]
    );
    assert_eq!(
        backlinks(&index, &vault, "keep/b.md"),
        [pair("keep/a.md", &["[[keep/b]]"])]
    );
    assert_eq!(
        backlinks(&index, &vault, "keep/c.md"),
        [pair("keep/b.md", &["[[keep/c]]"])]
    );
    for &gone in &gone {
        assert!(!index.is_indexed(gone));
    }
    for rows in index.by_stem.values() {
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|(source, _)| !gone.contains(source)));
    }
}

#[test]
fn a_source_for_an_unknown_note_is_ignored() {
    let (_fs, vault) = vault(&["Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, inkra_vault::EntryId::DETACHED, "[[Plan]]");

    assert!(!index.is_indexed(inkra_vault::EntryId::DETACHED));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
}

#[test]
fn backlinks_of_a_folder_or_unknown_id_are_empty() {
    let (_fs, vault) = vault(&["work/Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "work/Plan.md"), "[[work]]");

    assert_eq!(index.backlinks(id(&vault, "work")), []);
    assert_eq!(index.backlinks(inkra_vault::EntryId::DETACHED), []);
}

#[test]
fn resolve_link_parses_then_resolves() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let found = links("[[Plan]] <https://x.org>");
    let src = id(&vault, "src.md");

    assert_eq!(
        index.resolve_link(src, &found[0]),
        Resolution::Note(id(&vault, "Plan.md"))
    );
    assert_eq!(
        index.resolve_link(src, &found[1]),
        Resolution::External("https://x.org".to_owned())
    );
}

#[test]
fn an_insert_after_its_note_left_leaves_no_links_and_no_text() {
    let (fs, mut vault) = vault(&["src.md", "Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let src = id(&vault, "src.md");

    fs.remove(std::path::Path::new(&format!(
        "{}/src.md",
        crate::test_support::ROOT
    )));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("rescan");
    };
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, src, "[[Plan]]");

    assert!(!index.is_indexed(src));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert_eq!(texts(&index.corpus()), []);
}

/// The paths of the notes whose name matches `query`.
fn names(index: &NoteIndex, query: &str) -> Vec<String> {
    let snapshot = index.snapshot();
    let query = NoteQuery::new(query, None, &[]);
    let cancel = AtomicBool::new(false);
    let Some(lookup) = snapshot.lookup(&query, &cancel) else {
        panic!("not cancelled");
    };
    let kept = find_notes(
        &snapshot,
        &lookup,
        0..index.names().len(),
        &query,
        &cancel,
        10,
    );
    merge_notes(&snapshot, vec![kept], &query, 10)
        .into_iter()
        .filter(|found| found.group == Group::Name)
        .map(|found| found.path.to_string())
        .collect()
}

#[test]
fn names_follow_the_last_set_notes() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    assert!(index.names().is_empty());

    let _ = index.set_notes(note_set(&vault));
    assert_eq!(index.names().len(), 2);
    assert_eq!(names(&index, "b"), ["b"]);

    rename(&fs, &mut vault, "b.md", "c.md");
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(names(&index, "b"), Vec::<String>::new());
    assert_eq!(names(&index, "c"), ["c"]);
}

#[test]
fn the_same_relative_link_from_two_folders_names_two_notes() {
    let (_fs, vault) = vault(&["a/src.md", "a/Plan.md", "b/src.md", "b/Plan.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "a/src.md"), "[x](Plan.md)");
    load(&mut index, id(&vault, "b/src.md"), "[x](Plan.md)");

    assert_eq!(
        backlinks(&index, &vault, "a/Plan.md"),
        [pair("a/src.md", &["[x](Plan.md)"])]
    );
    assert_eq!(
        backlinks(&index, &vault, "b/Plan.md"),
        [pair("b/src.md", &["[x](Plan.md)"])]
    );
}

#[test]
fn set_notes_returns_the_previous_set() {
    let (_fs, two) = vault(&["a.md", "b.md"]);
    let (_fs, three) = vault(&["a.md", "b.md", "c.md"]);
    let mut index = NoteIndex::default();

    assert!(index.set_notes(note_set(&two)).names.is_empty());
    let previous = index.set_notes(note_set(&three));
    assert_eq!(previous.names.len(), 2);
    let ids: Vec<_> = entries(&two).into_iter().map(|(id, ..)| id).collect();
    assert_eq!(
        *previous.ids, *ids,
        "the previous set's ids, for the caller to free"
    );
    assert_eq!(index.names().len(), 3);

    let previous = index.set_notes(previous);
    assert_eq!(
        previous.names.len(),
        3,
        "a returned set can be applied again"
    );
    assert_eq!(*previous.mtimes, *note_set(&three).mtimes);
    assert_eq!(index.names().len(), 2);
}
