use std::sync::atomic::AtomicBool;

use inkra_vault::EntryId;

use crate::test_support::{entries, id, vault};
use crate::{NameMatch, NameQuery, NoteSet, match_names, merge_names};

const LIMIT: usize = 100;

fn run(files: &[&str], query: &str, showing: Option<&str>, recent: &[&str]) -> Vec<NameMatch> {
    let (_fs, vault) = vault(files);
    let names = NoteSet::build(entries(&vault)).names();
    let showing = showing.map(|path| id(&vault, path));
    let recent: Vec<EntryId> = recent.iter().map(|path| id(&vault, path)).collect();
    let query = NameQuery::new(query, showing, &recent);
    match_names(
        &names,
        0..names.len(),
        &query,
        &AtomicBool::new(false),
        LIMIT,
    )
}

fn ranked(files: &[&str], query: &str, showing: Option<&str>, recent: &[&str]) -> Vec<String> {
    run(files, query, showing, recent)
        .into_iter()
        .map(|found| found.path.to_string())
        .collect()
}

fn found(files: &[&str], query: &str) -> Vec<String> {
    ranked(files, query, None, &[])
}

fn only(files: &[&str], query: &str) -> NameMatch {
    let mut all = run(files, query, None, &[]);
    assert_eq!(all.len(), 1, "one match for {query:?}");
    all.remove(0)
}

#[test]
fn every_term_must_be_a_subsequence_in_any_order() {
    let files = ["work/plan.md", "home/list.md"];
    assert_eq!(found(&files, "pln"), ["work/plan"]);
    assert_eq!(found(&files, "plan work"), ["work/plan"]);
    assert_eq!(found(&files, "work plan"), ["work/plan"]);
    assert!(found(&files, "planx").is_empty());
}

#[test]
fn folders_are_not_candidates_and_the_extension_is_not_matched() {
    assert_eq!(found(&["plan/x.md"], "plan"), ["plan/x"]);
    assert!(found(&["note.md"], "md").is_empty());
}

#[test]
fn case_folds_and_accents_fold_one_way() {
    assert_eq!(found(&["Plan.md"], "PLAN"), ["Plan"]);
    assert_eq!(found(&["été.md"], "ete"), ["été"]);
    assert_eq!(found(&["Été.md"], "ETE"), ["Été"]);
    assert!(found(&["ete.md"], "été").is_empty());
    assert_eq!(found(&["été.md"], "e\u{301}te\u{301}"), ["été"]);
    assert_eq!(found(&["kilo.md"], "\u{212A}"), ["kilo"]);
}

#[test]
fn an_nfd_file_name_is_found_and_highlighted_on_its_nfc_path() {
    let found = only(&["dossier/cafe\u{301}.md"], "cafe");
    assert_eq!(found.path.as_ref(), "dossier/café");
    assert_eq!(found.positions, [8, 9, 10, 11]);
}

#[test]
fn positions_skip_multi_byte_chars() {
    assert_eq!(only(&["🎉 party.md"], "party").positions, [5, 6, 7, 8, 9]);
    assert_eq!(only(&["🎉 party.md"], "🎉").positions, [0]);
}

#[test]
fn positions_of_several_terms_are_merged_in_order() {
    assert_eq!(
        only(&["work/plan.md"], "plan work").positions,
        [0, 1, 2, 3, 5, 6, 7, 8]
    );
}

#[test]
fn a_stem_match_outranks_a_folder_match() {
    assert_eq!(
        found(&["plan/notes.md", "work/plan.md"], "plan"),
        ["work/plan", "plan/notes"]
    );
}

#[test]
fn an_exact_stem_outranks_a_shorter_path_that_only_contains_it() {
    assert_eq!(
        found(&["planning.md", "a/b/plan.md"], "plan"),
        ["a/b/plan", "planning"]
    );
}

#[test]
fn consecutive_chars_outrank_scattered_ones() {
    assert_eq!(found(&["cxaxt.md", "catxx.md"], "cat"), ["catxx", "cxaxt"]);
}

#[test]
fn a_word_start_or_case_change_outranks_a_mid_word_match() {
    assert_eq!(found(&["xcat.md", "x-cat.md"], "cat"), ["x-cat", "xcat"]);
    assert_eq!(found(&["xcat.md", "xCat.md"], "cat"), ["xCat", "xcat"]);
}

#[test]
fn a_longer_gap_costs_more_until_the_cap() {
    let score = |file: &str| only(&[file], "az").score;
    assert!(score("axzxxxxx.md") > score("axxxxxzx.md"));
    let long_a = format!("a{}z.md", "x".repeat(20));
    let long_b = format!("a{}z.md", "x".repeat(30));
    assert_eq!(score(&long_a), score(&long_b));
}

#[test]
fn scores_follow_the_constants() {
    let score = |file: &str, query: &str| only(&[file], query).score;
    // 16 + 24 + 32 for `p`, 32 for each consecutive char, 8 per char in the stem, 1024 exact.
    assert_eq!(score("plan.md", "plan"), 1224);
    assert_eq!(score("planning.md", "plan"), 200);
    // A boundary after `-` gains 24; a mid-path char gains no stem-start bonus.
    assert_eq!(score("x-cat.md", "cat"), 128);
    // The folder's `plan` (136) beats the scattered stem's (55 + 32).
    assert_eq!(score("plan/xpxlxaxn.md", "plan"), 136);
    // The stem's `a` (72) has no `b` after it, so it scores nothing.
    assert_eq!(score("xaxb/a.md", "ab"), 29);
}

#[test]
fn positions_follow_the_best_alignment_not_the_nearest_match() {
    let found = only(&["axab.md"], "ab");
    assert_eq!(found.score, 100);
    assert_eq!(found.positions, [0, 3]);
}

#[test]
fn the_note_showing_loses_a_quarter_of_its_score() {
    let files = ["plan a.md", "plan b.md"];
    assert_eq!(ranked(&files, "plan", None, &[]), ["plan a", "plan b"]);
    assert_eq!(
        ranked(&files, "plan", Some("plan a.md"), &[]),
        ["plan b", "plan a"]
    );
    let full = run(&files, "plan", None, &[])[0].score;
    let showing = run(&files, "plan", Some("plan a.md"), &[])[1].score;
    assert_eq!(showing, full - full / 4);
}

#[test]
fn ties_break_by_recency_then_path_length_then_vault_order() {
    let files = ["plan a.md", "plan b.md", "x/plan a.md"];
    assert_eq!(
        ranked(&files, "plan", None, &[]),
        ["plan a", "plan b", "x/plan a"]
    );
    assert_eq!(
        ranked(&files, "plan", None, &["x/plan a.md", "plan b.md"]),
        ["x/plan a", "plan b", "plan a"]
    );
}

#[test]
fn an_empty_query_lists_recent_notes_without_the_one_showing() {
    let files = ["a.md", "b.md", "c.md"];
    assert_eq!(
        ranked(&files, "", Some("a.md"), &["a.md", "c.md", "b.md"]),
        ["c", "b"]
    );
    assert_eq!(ranked(&files, "   ", None, &["b.md"]), ["b"]);
    assert!(ranked(&files, "", None, &[]).is_empty());
}

#[test]
fn a_recent_note_no_longer_in_the_vault_is_skipped() {
    let (_fs, vault) = vault(&["a.md"]);
    let names = NoteSet::build(entries(&vault)).names();
    let query = NameQuery::new("", None, &[EntryId::DETACHED, id(&vault, "a.md")]);
    let found = match_names(&names, 0..1, &query, &AtomicBool::new(false), LIMIT);
    assert_eq!(found.len(), 1);
}

#[test]
fn the_limit_keeps_the_best_matches() {
    let files = ["n1.md", "x/n2.md", "n3.md", "y/y/n4.md"];
    let (_fs, vault) = vault(&files);
    let names = NoteSet::build(entries(&vault)).names();
    let query = NameQuery::new("n", None, &[]);
    let all = match_names(&names, 0..4, &query, &AtomicBool::new(false), LIMIT);
    let two = match_names(&names, 0..4, &query, &AtomicBool::new(false), 2);
    assert_eq!(two, all[..2]);
    assert!(match_names(&names, 0..4, &query, &AtomicBool::new(false), 0).is_empty());
}

#[test]
fn a_query_longer_than_every_path_finds_nothing() {
    assert!(found(&["a.md"], &"a".repeat(300)).is_empty());
}

#[test]
fn a_set_cancel_flag_returns_nothing() {
    let (_fs, vault) = vault(&["plan.md"]);
    let names = NoteSet::build(entries(&vault)).names();
    let query = NameQuery::new("plan", None, &[]);
    assert!(match_names(&names, 0..1, &query, &AtomicBool::new(true), LIMIT).is_empty());
}

#[test]
fn merging_split_results_equals_one_range() {
    let files: Vec<String> = (0..40).map(|n| format!("f{}/plan {n}.md", n % 7)).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let names = NoteSet::build(entries(&vault)).names();
    let query = NameQuery::new("pl 3", None, &[]);
    let cancel = AtomicBool::new(false);
    let whole = match_names(&names, 0..names.len(), &query, &cancel, 5);
    let parts = names
        .chunks(4)
        .into_iter()
        .map(|range| match_names(&names, range, &query, &cancel, 5))
        .collect();
    assert_eq!(merge_names(parts, 5), whole);
}

#[test]
fn an_exact_multi_word_stem_outranks_a_recent_note_that_only_contains_it() {
    assert_eq!(
        ranked(&["plan.md", "planning.md"], "plan", None, &["planning.md"]),
        ["plan", "planning"],
        "one word: the exact stem wins over recency"
    );
    let files = ["meeting notes.md", "meeting notes draft.md"];
    assert_eq!(
        ranked(&files, "meeting notes", None, &["meeting notes draft.md"]),
        ["meeting notes", "meeting notes draft"],
        "several words: the stem equals the query, so it gains the same bonus"
    );
}

#[test]
fn a_deep_exact_multi_word_stem_outranks_a_shorter_path_that_only_contains_it() {
    let files = [
        "meeting notes 2.md",
        "meeting notes draft.md",
        "x/y/meeting notes.md",
    ];
    let expected = [
        "x/y/meeting notes",
        "meeting notes 2",
        "meeting notes draft",
    ];
    assert_eq!(found(&files, "meeting notes"), expected);
    assert_eq!(found(&files, "meeting   notes"), expected);
}

#[test]
fn a_repeated_term_counts_once() {
    let score = |query: &str| only(&["x/plans.md"], query).score;
    assert_eq!(score("plan plan"), score("plan"));
    assert_eq!(score("Plan plan"), score("plan"));
    assert_eq!(score("plan x plan"), score("plan x"));
    assert_eq!(
        only(&["x/plans.md"], "plan plan").positions,
        only(&["x/plans.md"], "plan").positions
    );
}

#[test]
fn the_exact_stem_compares_the_query_before_repeats_are_dropped() {
    let score = |query: &str| only(&["plan plan.md"], query).score;
    assert_eq!(score("plan plan"), score("plan") + 1024);
}

#[test]
fn a_long_query_is_cut_to_its_first_64_chars() {
    let files = [
        "abcdefghijklmnopqrstuvwxyzé.md",
        "x/abcdefghijklmnopqrstuvwxyzé.md",
        "abcdefghijklmnopqrstuvwxyz.md",
    ];
    let pairs: Vec<String> = ('b'..='v').map(|c| format!("a{c}")).collect();
    let tail: Vec<String> = (0..80).map(|n| format!("q{n}")).collect();
    // `éz` straddles the cut: its `é` is the 64th char and matches, `z` after it would not.
    let text = format!("{} éz {}", pairs.join(" "), tail.join(" "));
    let cut: String = text.chars().take(64).collect();
    assert!(text.chars().count() >= 300);
    assert!(cut.ends_with('é'));
    let long = run(&files, &text, None, &[]);
    assert_eq!(long.len(), 2);
    assert_eq!(long, run(&files, &cut, None, &[]));
}

#[test]
fn a_repeated_recent_id_keeps_its_newest_rank() {
    let files = ["plan a.md", "plan b.md"];
    assert_eq!(
        ranked(
            &files,
            "plan",
            None,
            &["plan b.md", "plan a.md", "plan b.md"]
        ),
        ["plan b", "plan a"]
    );
}

fn assert_send_sync<T: Send + Sync + 'static>() {}

#[test]
fn the_matcher_types_cross_threads() {
    assert_send_sync::<crate::NamesSnapshot>();
    assert_send_sync::<NameQuery>();
    assert_send_sync::<NameMatch>();
    assert_send_sync::<NoteSet>();
}
