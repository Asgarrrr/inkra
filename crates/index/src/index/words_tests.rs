use std::collections::BTreeMap;

use inkra_vault::EntryId;

use super::NoteIndex;
use crate::MergeWhen;
use crate::test_support::{ROOT, id, load, note_set, rename, vault};
use crate::words::FRESH_MIN;

fn entries(index: &NoteIndex) -> BTreeMap<(String, EntryId), u32> {
    index.words().entries()
}

#[test]
fn a_load_indexes_the_words_and_a_reload_replaces_them() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    load(&mut index, a, "Été [[plan]]");
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("ete".to_owned(), a), 1), (("plan".to_owned(), a), 1)])
    );
    load(&mut index, a, "other");
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("other".to_owned(), a), 1)])
    );
}

#[test]
fn remove_drops_the_words() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    load(&mut index, a, "plan");
    index.remove(a);
    assert!(entries(&index).is_empty());
}

#[test]
fn a_load_for_a_non_note_indexes_nothing() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, EntryId::DETACHED, "plan");
    assert!(entries(&index).is_empty());
}

#[test]
fn a_rename_keeps_the_words_and_a_trash_drops_them() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    load(&mut index, a, "alpha");
    load(&mut index, b, "beta");

    rename(&fs, &mut vault, "a.md", "c.md");
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("alpha".to_owned(), a), 1), (("beta".to_owned(), b), 1)])
    );

    fs.remove(std::path::Path::new(&format!("{ROOT}/b.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("alpha".to_owned(), a), 1)])
    );
}

#[test]
fn a_load_finishing_after_its_note_left_indexes_nothing() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let b = id(&vault, "b.md");
    fs.remove(std::path::Path::new(&format!("{ROOT}/b.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, b, "late");
    assert!(entries(&index).is_empty());
}

#[test]
fn a_due_merge_runs_off_the_index_and_a_remove_during_it_stays() {
    let files: Vec<String> = (0..=FRESH_MIN).map(|n| format!("n{n:03}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
    for &id in &ids[..FRESH_MIN - 1] {
        load(&mut index, id, "plan");
    }
    assert!(index.words_merge(MergeWhen::Due).is_none(), "not due yet");
    load(&mut index, ids[FRESH_MIN - 1], "plan");
    let Some(merge) = index.words_merge(MergeWhen::Due) else {
        panic!("FRESH_MIN fresh notes are due");
    };
    assert!(
        index.words_merge(MergeWhen::Due).is_none(),
        "one merge at a time"
    );
    index.remove(ids[0]);
    load(&mut index, ids[FRESH_MIN], "late");
    let before = entries(&index);
    let _ = index.words_apply(merge.run());
    assert_eq!(entries(&index), before);
    assert!(!before.contains_key(&("plan".to_owned(), ids[0])));
    assert_eq!(before.get(&("late".to_owned(), ids[FRESH_MIN])), Some(&1));
    assert!(
        index.words_merge(MergeWhen::Due).is_none(),
        "two fresh notes are not due"
    );
}

#[test]
fn a_trash_during_a_running_merge_stays_trashed_after_it_applies() {
    let (fs, mut vault) = vault(&["a.md", "b.md", "c.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b, c) = (id(&vault, "a.md"), id(&vault, "b.md"), id(&vault, "c.md"));
    load(&mut index, a, "merged");
    index.merge_all();
    load(&mut index, b, "fresh");
    load(&mut index, c, "kept");
    let Some(merge) = index.words.start_merge() else {
        panic!("b and c are fresh");
    };
    for file in ["a.md", "b.md"] {
        fs.remove(std::path::Path::new(&format!("{ROOT}/{file}")));
    }
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    let _ = index.words_apply(merge.run());
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("kept".to_owned(), c), 1)])
    );
}

#[test]
fn a_settle_merges_fresh_notes_below_the_policy_one_merge_at_a_time() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    assert!(index.words_settled());
    assert!(
        index.words_merge(MergeWhen::Settle).is_none(),
        "nothing to settle"
    );
    load(&mut index, a, "alpha");
    assert!(
        index.words_merge(MergeWhen::Due).is_none(),
        "one fresh note is not due"
    );
    assert!(!index.words_settled());
    let Some(merge) = index.words_merge(MergeWhen::Settle) else {
        panic!("a fresh note settles");
    };
    assert!(
        index.words_merge(MergeWhen::Settle).is_none(),
        "one merge at a time"
    );
    assert!(!index.words_settled(), "a merge runs");
    load(&mut index, b, "beta");
    let before = entries(&index);
    let _ = index.words_apply(merge.run());
    assert_eq!(entries(&index), before);
    assert!(
        !index.words_settled(),
        "b, written during the merge, stays fresh"
    );
    let Some(merge) = index.words_merge(MergeWhen::Settle) else {
        panic!("b settles");
    };
    let _ = index.words_apply(merge.run());
    assert!(index.words_settled());
    assert_eq!(entries(&index), before);
    assert!(index.words_merge(MergeWhen::Settle).is_none(), "settled");
    index.remove(a);
    assert!(index.words_settled(), "a dead note is not fresh");
    assert!(
        index.words_merge(MergeWhen::Settle).is_some(),
        "half the segment dead is due"
    );
}

#[test]
fn a_reload_keeps_the_note_s_ordinal_and_a_remove_gives_it_a_new_one() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    load(&mut index, a, "plan");
    load(&mut index, b, "plan");
    for text in ["other", "more"] {
        load(&mut index, a, text);
    }
    index.merge_all();
    load(&mut index, a, "again");
    assert_eq!(index.words().ordinal_span(), 2);
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("again".to_owned(), a), 1), (("plan".to_owned(), b), 1)])
    );
    index.remove(a);
    load(&mut index, a, "back");
    assert_eq!(index.words().ordinal_span(), 3);
}

#[test]
fn a_load_that_empties_a_note_keeps_its_ordinal() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    load(&mut index, a, "plan");
    load(&mut index, b, "plan");
    for text in ["", "again", "", "last"] {
        load(&mut index, a, text);
        index.merge_all();
    }
    assert_eq!(index.words().ordinal_span(), 2);
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("last".to_owned(), a), 1), (("plan".to_owned(), b), 1)])
    );
}

#[test]
fn a_note_emptied_then_gone_frees_its_ordinal() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    load(&mut index, a, "plan");
    load(&mut index, b, "plan");
    load(&mut index, a, "");
    index.merge_all();
    let held = index.words().heap_bytes();

    fs.remove(std::path::Path::new(&format!("{ROOT}/a.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));

    // An emptied note holds no word, so only its entry in the ordinal map can leave.
    assert_eq!(held - index.words().heap_bytes(), 2 * size_of::<u32>());
}
