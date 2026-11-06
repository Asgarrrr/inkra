use std::sync::atomic::AtomicBool;

use futures::executor::block_on;
use inkra_vault::{EntryId, Serial, Vault};
use unicode_normalization::UnicodeNormalization;

use crate::find::find_in;
use crate::test_support::{content, id, mtime, note_set, vault};
use crate::{Group, NoteIndex, NoteMatch, NoteQuery};

/// Texts load at `mtime(0)`, so mtimes tie unless a test sets them.
fn with_texts(files: &[(&str, Option<&str>)]) -> (Vault, NoteIndex) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let (_fs, vault) = vault(&names);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    for (name, text) in files {
        if let Some(text) = text {
            index.insert(id(&vault, name), content(*text, mtime(0)));
        }
    }
    (vault, index)
}

pub(super) fn with_names(files: &[&str]) -> (Vault, NoteIndex) {
    let files: Vec<(&str, Option<&str>)> = files.iter().map(|name| (*name, Some(""))).collect();
    with_texts(&files)
}

fn run(index: &NoteIndex, query: &NoteQuery, cancel: bool, limit: usize) -> Vec<NoteMatch> {
    let cancel = AtomicBool::new(cancel);
    block_on(index.snapshot().find_notes(query, limit, &Serial, &cancel)).unwrap_or_default()
}

pub(super) fn find(index: &NoteIndex, text: &str) -> Vec<NoteMatch> {
    run(index, &NoteQuery::new(text, None, &[]), false, usize::MAX)
}

fn paths(found: &[NoteMatch]) -> Vec<&str> {
    found.iter().map(|found| &*found.path).collect()
}

fn groups(found: &[NoteMatch]) -> Vec<(&str, Group)> {
    found
        .iter()
        .map(|found| (&*found.path, found.group))
        .collect()
}

fn ids(vault: &Vault, files: &[&str]) -> Vec<EntryId> {
    files.iter().map(|file| id(vault, file)).collect()
}

#[test]
fn every_term_must_match_a_word_start() {
    let (_vault, index) = with_texts(&[("Planning.md", Some("")), ("b.md", Some("annual plan"))]);
    assert_eq!(paths(&find(&index, "plan")), ["Planning", "b"]);
    assert_eq!(
        paths(&find(&index, "ann")),
        ["b"],
        "`Planning` holds no word starting with it"
    );
    assert_eq!(paths(&find(&index, "plan zzz")), Vec::<&str>::new());
}

#[test]
fn the_three_groups_rank_in_order() {
    let (_vault, index) = with_texts(&[
        ("a.md", Some("plan work")),
        ("b plan.md", Some("work")),
        ("c plan work.md", Some("")),
    ]);
    assert_eq!(
        groups(&find(&index, "plan work")),
        [
            ("c plan work", Group::Name),
            ("b plan", Group::NameAndText),
            ("a", Group::Text)
        ]
    );
}

#[test]
fn whole_terms_beat_prefixes() {
    let (_vault, index) = with_names(&["planning.md", "zzzz plan.md"]);
    assert_eq!(paths(&find(&index, "plan")), ["zzzz plan", "planning"]);
    let (_vault, index) = with_texts(&[("a.md", Some("planning")), ("b.md", Some("plan"))]);
    assert_eq!(paths(&find(&index, "plan")), ["b", "a"], "in the text too");
}

#[test]
fn stem_terms_beat_folder_terms() {
    let (_vault, index) = with_names(&["plan/aaa.md", "zzz/plan.md"]);
    assert_eq!(paths(&find(&index, "plan")), ["zzz/plan", "plan/aaa"]);
    let (_vault, index) = with_names(&["plan/aaa b.md", "zzz/plan b.md"]);
    assert_eq!(
        paths(&find(&index, "plan")),
        ["zzz/plan b", "plan/aaa b"],
        "neither is exact"
    );
}

#[test]
fn more_occurrences_rank_first_in_the_text_groups() {
    let (_vault, index) = with_texts(&[
        ("a.md", Some("plan")),
        ("b.md", Some("plan plan")),
        ("c work.md", Some("plan")),
        ("d work.md", Some("plans plan")),
    ]);
    assert_eq!(
        groups(&find(&index, "plan work")),
        [
            ("d work", Group::NameAndText),
            ("c work", Group::NameAndText)
        ]
    );
    assert_eq!(paths(&find(&index, "plan")), ["b", "d work", "a", "c work"]);
}

#[test]
fn the_note_showing_ranks_last_in_its_group() {
    let (vault, index) = with_texts(&[
        ("a plan.md", Some("")),
        ("b plan.md", Some("")),
        ("c.md", Some("plan")),
    ]);
    let query = NoteQuery::new("plan", Some(id(&vault, "a plan.md")), &[]);
    assert_eq!(
        paths(&run(&index, &query, false, usize::MAX)),
        ["b plan", "a plan", "c"]
    );
}

#[test]
fn recent_notes_rank_first_newest_first() {
    let files = ["a plan.md", "b plan.md", "c plan.md"];
    let (vault, index) = with_names(&files);
    let [a, b, c] = ids(&vault, &files)[..] else {
        panic!("three notes");
    };
    let query = NoteQuery::new("plan", None, &[c, b, c, a]);
    assert_eq!(
        paths(&run(&index, &query, false, usize::MAX)),
        ["c plan", "b plan", "a plan"]
    );
}

/// The paths `text` lists; `recent` is newest first and newer by mtime than the other notes.
fn ranked(files: &[&str], text: &str, recent: &[&str]) -> Vec<String> {
    let (vault, mut index) = with_names(files);
    for file in recent {
        index.insert(id(&vault, file), content("", mtime(9)));
    }
    let query = NoteQuery::new(text, None, &ids(&vault, recent));
    let found = run(&index, &query, false, usize::MAX);
    paths(&found).into_iter().map(str::to_owned).collect()
}

#[test]
fn the_exact_name_ranks_first() {
    let others = [
        "plan b.md",
        "release plan.md",
        "a plan whose name is long.md",
    ];
    let files = [&["plan.md"][..], &others].concat();
    assert_eq!(
        ranked(&files, "plan", &others),
        [
            "plan",
            "plan b",
            "release plan",
            "a plan whose name is long"
        ]
    );
    assert_eq!(
        ranked(&["Plan.md", "plan b.md"], "plân", &["plan b.md"]),
        ["Plan", "plan b"]
    );
    let files = ["Plan - B.md", "b plan.md", "plan b c.md"];
    assert_eq!(
        ranked(&files, "plan b", &files[1..]),
        ["Plan - B", "b plan", "plan b c"]
    );
    let files = ["plan.md", "plan plan.md"];
    assert_eq!(ranked(&files, "plan", &files[1..]), ["plan", "plan plan"]);
    assert_eq!(
        ranked(&files, "plan plan", &files[..1]),
        ["plan plan", "plan"]
    );
    let files = ["plan/notes.md", "x plan/notes.md"];
    assert_eq!(
        ranked(&files, "plan", &files[1..]),
        ["x plan/notes", "plan/notes"],
        "a folder is not the name"
    );
    assert_eq!(
        ranked(&files, "plan notes", &files[1..]),
        ["x plan/notes", "plan/notes"]
    );
    // A 65-char stem word is stored at 65 chars, the query cut to 64: never exact.
    let (short, long) = ("a".repeat(64), "a".repeat(65));
    let files = [format!("{short}.md"), format!("{long}.md")];
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    assert_eq!(ranked(&files, &long, &files[1..]), [short, long]);
    // U+0958 folds to two chars, so its 64 typed chars make a term capped at 64 folded chars.
    let (short, long) = ("\u{958}".repeat(32), "\u{958}".repeat(64));
    let files = [format!("{short}.md"), format!("{long}.md")];
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let nfc = |name: &str| -> String { name.nfc().collect() };
    assert_eq!(
        ranked(&files, &long, &files[1..]),
        [nfc(&short), nfc(&long)]
    );
    let files = ["a.md", "🎉.md"];
    assert_eq!(
        ranked(&files, "", &files),
        ["a", "🎉"],
        "an empty query is never exact"
    );
}

#[test]
fn the_newest_mtime_ranks_first() {
    let files = ["a.md", "b.md", "c.md"];
    let (vault, mut index) = with_names(&files);
    for (file, n) in files.iter().zip([5, 9, 7]) {
        index.insert(id(&vault, file), content("plan", mtime(n)));
    }
    assert_eq!(paths(&find(&index, "plan")), ["b", "c", "a"]);
}

#[test]
fn a_shorter_path_ranks_first_then_vault_order() {
    let (_vault, index) = with_names(&["plan a1.md", "plan y.md", "plan z.md"]);
    assert_eq!(
        paths(&find(&index, "plan")),
        ["plan y", "plan z", "plan a1"]
    );
}

#[test]
fn a_name_row_never_ranks_by_its_text() {
    let many = "plan ".repeat(50);
    let (_vault, index) = with_texts(&[("plan/x.md", Some(&many)), ("y/plan.md", Some(""))]);
    assert_eq!(
        groups(&find(&index, "plan")),
        [("y/plan", Group::Name), ("plan/x", Group::Name)]
    );
}

#[test]
fn accents_fold_both_ways_in_names() {
    let (_vault, index) = with_names(&["Été.md", "ete x.md"]);
    assert_eq!(paths(&find(&index, "ete")), ["Été", "ete x"]);
    assert_eq!(paths(&find(&index, "été")), ["Été", "ete x"]);
    assert_eq!(paths(&find(&index, "ÉTÉ")), ["Été", "ete x"]);
}

#[test]
fn an_empty_query_lists_the_recent_notes_without_the_one_showing() {
    let files = ["a.md", "b.md", "c.md", "d.md"];
    let (vault, index) = with_names(&files);
    let [a, b, c, _] = ids(&vault, &files)[..] else {
        panic!("four notes");
    };
    let query = NoteQuery::new(" ", Some(b), &[c, b, a]);
    assert!(query.is_empty());
    let found = run(&index, &query, false, usize::MAX);
    assert_eq!(groups(&found), [("c", Group::Name), ("a", Group::Name)]);
    assert!(found.iter().all(|found| found.name_highlights.is_empty()));
}

#[test]
fn a_cancelled_search_lists_nothing() {
    let (_vault, index) = with_names(&["plan.md"]);
    let query = NoteQuery::new("plan", None, &[]);
    let snapshot = index.snapshot();
    let Some(lookup) = snapshot.lookup(&query, &AtomicBool::new(false)) else {
        panic!("not cancelled");
    };
    let found = find_in(
        &snapshot,
        &lookup,
        0..index.names().len(),
        &query,
        &AtomicBool::new(true),
        usize::MAX,
    );
    assert_eq!(found, []);
}

#[test]
fn limit_zero_lists_nothing() {
    let (_vault, index) = with_names(&["plan.md", "plan b.md"]);
    let query = NoteQuery::new("plan", None, &[]);
    assert_eq!(run(&index, &query, false, 0), []);
    assert_eq!(paths(&run(&index, &query, false, 1)), ["plan"]);
}

#[test]
fn a_note_not_loaded_is_found_by_its_name_only() {
    let (_vault, index) = with_texts(&[("plan.md", None), ("other.md", None)]);
    assert_eq!(groups(&find(&index, "plan")), [("plan", Group::Name)]);
    assert_eq!(paths(&find(&index, "plan other")), Vec::<&str>::new());
}

#[test]
fn a_rare_term_outweighs_a_common_one() {
    let filler = |n: usize| "mot ".repeat(n);
    let many_common = format!("{}kubernetes", "projet ".repeat(40));
    let many_rare = format!(
        "{}{}{}",
        "projet ".repeat(2),
        "kubernetes ".repeat(8),
        filler(31)
    );
    let others: Vec<(String, String)> = (0..10)
        .map(|n| (format!("c{n}.md"), format!("projet {}", filler(40))))
        .collect();
    let mut files = vec![("a.md", Some(&*many_common)), ("b.md", Some(&*many_rare))];
    files.extend(others.iter().map(|(name, text)| (&**name, Some(&**text))));
    let (_vault, index) = with_texts(&files);
    assert_eq!(paths(&find(&index, "projet kubernetes")), ["b", "a"]);
}

#[test]
fn at_one_length_the_rarer_term_decides() {
    let a = format!("{}kubernetes {}", "projet ".repeat(3), "mot ".repeat(36));
    let b = format!("projet {}{}", "kubernetes ".repeat(2), "mot ".repeat(37));
    let filler = format!("projet {}", "mot ".repeat(39));
    let others: Vec<String> = (0..10).map(|n| format!("c{n}.md")).collect();
    let mut files = vec![("a.md", Some(&*a)), ("b.md", Some(&*b))];
    files.extend(others.iter().map(|name| (&**name, Some(&*filler))));
    let (_vault, index) = with_texts(&files);
    assert_eq!(paths(&find(&index, "projet kubernetes")), ["b", "a"]);
}

#[test]
fn a_short_note_outranks_a_long_one_with_as_many_occurrences() {
    let long = format!("{}{}", "graphe ".repeat(3), "mot ".repeat(2997));
    let short = format!("{}{}", "graphe ".repeat(3), "mot ".repeat(27));
    let (_vault, index) = with_texts(&[("a.md", Some(&long)), ("b.md", Some(&short))]);
    assert_eq!(paths(&find(&index, "graphe")), ["b", "a"]);
}

#[test]
fn more_occurrences_still_rank_first_at_one_length() {
    let few = format!("{}{}", "graphe ".repeat(3), "mot ".repeat(297));
    let many = "graphe ".repeat(300);
    let (_vault, index) = with_texts(&[("a.md", Some(&few)), ("b.md", Some(&many))]);
    assert_eq!(paths(&find(&index, "graphe")), ["b", "a"]);
}

#[test]
fn matches_differing_only_in_key_2_are_not_equal() {
    let (_vault, once) = with_texts(&[("a.md", Some("plan x"))]);
    let (_vault, twice) = with_texts(&[("a.md", Some("plan plan"))]);
    let (once, twice) = (find(&once, "plan"), find(&twice, "plan"));
    assert_eq!(once[0].id, twice[0].id);
    assert_eq!(groups(&once), groups(&twice));
    assert_ne!(once, twice, "the score is part of the match");
}
