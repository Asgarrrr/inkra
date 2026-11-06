use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::Vault;
use proptest::prelude::*;

use super::excerpt::{EXCERPT_SCAN_BYTES, around_match};
use super::terms::{Terms, each_word};
use crate::test_support::{PARTS, find_all, id, load, note_set, vault};
use crate::{Excerpt, NoteIndex, NoteMatch, NoteQuery, excerpt};

pub(super) fn with_texts(files: &[(&str, Option<&str>)]) -> (Vault, NoteIndex) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let (_fs, vault) = vault(&names);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    for (name, text) in files {
        if let Some(text) = text {
            load(&mut index, id(&vault, name), text);
        }
    }
    (vault, index)
}

pub(super) fn found(index: &NoteIndex, query: &NoteQuery) -> NoteMatch {
    let mut found = find_all(&index.snapshot(), query, usize::MAX);
    assert_eq!(found.len(), 1, "one note found");
    found.remove(0)
}

fn of(text: &str, query: &str) -> Option<Excerpt> {
    let (_vault, index) = with_texts(&[("n.md", Some(text))]);
    let query = NoteQuery::new(query, None, &[]);
    excerpt(
        &index.snapshot(),
        &found(&index, &query),
        &query,
        text,
        &AtomicBool::new(false),
    )
}

pub(super) fn shown(text: &str, query: &str) -> (String, Vec<(usize, usize)>) {
    let Some(Excerpt { text, highlights }) = of(text, query) else {
        panic!("an excerpt of {query:?}");
    };
    let highlights = highlights
        .into_iter()
        .map(|range| (range.start, range.end))
        .collect();
    (text, highlights)
}

fn lit(text: &str, query: &str) -> Vec<String> {
    let (text, highlights) = shown(text, query);
    highlights
        .into_iter()
        .map(|(start, end)| text[start..end].to_owned())
        .collect()
}

#[test]
fn a_text_row_shows_the_line_around_the_first_match() {
    let (text, highlights) = shown("intro\nsome words about planning\nlast plan", "plan");
    assert_eq!(text, "some words about planning");
    assert_eq!(highlights, [(17, 21)]);
}

#[test]
fn every_matched_word_in_the_window_is_lit_on_its_prefix() {
    assert_eq!(
        lit("Planning the plan, and PLANS; explain", "pla"),
        ["Pla", "pla", "PLA"]
    );
    assert_eq!(lit("Planning the plan", "pla the"), ["Pla", "the", "pla"]);
    assert_eq!(
        lit("planning", "pl pla"),
        ["pla"],
        "two terms on one word merge"
    );
    assert_eq!(
        lit("e\u{301}te\u{301}", "e"),
        ["e\u{301}"],
        "a whole grapheme"
    );
    assert_eq!(lit("plan-planning", "plan"), ["plan", "plan"]);
    assert_eq!(lit("planplan plan", "plan"), ["plan", "plan"]);
    assert_eq!(
        shown("plan plan", "plan").1,
        [(0, 4), (5, 9)],
        "ascending, apart"
    );
    assert_eq!(
        shown("a-\u{301}b", "a b").1,
        [(0, 5)],
        "touching ranges merge: `-\u{301}` is one grapheme"
    );
}

#[test]
fn a_match_at_a_line_start_and_at_a_line_end() {
    assert_eq!(
        shown("first\nplan rest\nlast", "plan"),
        ("plan rest".into(), vec![(0, 4)])
    );
    assert_eq!(
        shown("first\nsome plan\nlast", "plan"),
        ("some plan".into(), vec![(5, 9)])
    );
    assert_eq!(shown("plan", "plan"), ("plan".into(), vec![(0, 4)]));
}

#[test]
fn a_multibyte_text_cuts_on_char_boundaries() {
    let text = format!("{} λόγος {}", "🎉".repeat(20), "ω".repeat(60));
    let (shown, highlights) = shown(&text, "λογ");
    assert_eq!(
        shown,
        format!("{} λόγος {}", "🎉".repeat(14), "ω".repeat(49))
    );
    assert_eq!(
        highlights
            .into_iter()
            .map(|(start, end)| &shown[start..end])
            .collect::<Vec<_>>(),
        ["λόγ"]
    );
}

#[test]
fn a_ten_megabyte_line_gives_a_window_not_the_line() {
    // The match sits inside the scan limit; past it there is no excerpt.
    let before = "xy ".repeat(200_000);
    let after = "xy ".repeat(3_200_000);
    let (shown, highlights) = shown(&format!("{before}plan {after}"), "plan");
    assert_eq!(shown, format!("{}plan {}", &before[..60], &after[..99]));
    assert_eq!(highlights, [(60, 64)]);
}

#[test]
fn a_match_past_the_scan_limit_has_no_excerpt() {
    let text = format!("{}plan", "xy ".repeat((10 << 20) / 3));
    assert_eq!(of(&text, "plan"), None);

    let at = EXCERPT_SCAN_BYTES - 4;
    let text = format!("{}plan {}", "x ".repeat(at / 2), "x ".repeat(at));
    assert_eq!(lit(&text, "plan"), ["plan"], "a match ending on the limit");
}

#[test]
fn a_word_across_a_cancel_block_stays_one_word() {
    // `cancel` is read every 64 KB; `zzplan` straddles that edge and must not yield `plan`.
    let text = format!("{}zzplan plan", " ".repeat((64 << 10) - 2));
    let (shown, highlights) = shown(&text, "plan");
    assert_eq!(shown, format!("{}zzplan plan", " ".repeat(53)));
    assert_eq!(highlights, [(60, 64)]);
}

#[test]
fn a_word_ending_on_a_cancel_block_edge_stays_apart_from_the_next() {
    // `abc` ends exactly on the 64 KB edge; ` plan` in the next block is its own word.
    let text = format!("{}abc plan", " ".repeat((64 << 10) - 3));
    assert_eq!(lit(&text, "plan"), ["plan"]);
}

#[test]
fn a_cancel_set_during_the_scan_gives_none() {
    let query = NoteQuery::new("plan", None, &[]);
    let cancel = AtomicBool::new(true);
    assert_eq!(around_match("a plan", &Terms::new(&query), &cancel), None);
}

#[test]
fn the_scan_stops_at_the_block_edge_past_the_window() {
    // Cancelling on the first word shows whether a second block is read.
    let text = format!("plan {}", "-".repeat(1 << 20));
    let cancel = AtomicBool::new(false);
    let mut fed = Vec::new();
    let read = each_word(&text, &cancel, |word| {
        fed.push((word.start, word.end));
        cancel.store(true, Ordering::Relaxed);
        word.end
    });
    assert_eq!(read, Some(()), "no second block read");
    assert_eq!(fed, [(0, 4)]);
}

#[test]
fn a_bound_on_the_block_edge_reads_no_further_block() {
    // A word starting at the bound is never fed, so the block starting there is never read.
    let text = format!("plan {}", "-".repeat(1 << 20));
    let cancel = AtomicBool::new(false);
    let read = each_word(&text, &cancel, |_| {
        cancel.store(true, Ordering::Relaxed);
        64 << 10
    });
    assert_eq!(read, Some(()));
}

#[test]
fn a_one_megabyte_word_gives_a_short_excerpt() {
    let text = format!("see plan{} end", "x".repeat(1 << 20));
    let (shown, highlights) = shown(&text, "plan");
    assert_eq!(shown.len(), "see ".len() + 65 + 100);
    assert_eq!(highlights, [(4, 8)]);
}

#[test]
fn a_set_cancel_gives_none() {
    let (vault, index) = with_texts(&[("plan.md", Some("plan")), ("n.md", Some("a plan"))]);
    let texts = [("plan.md", "plan"), ("n.md", "a plan")];
    let query = NoteQuery::new("plan", None, &[]);
    let row = |name| {
        let all = find_all(&index.snapshot(), &query, 9);
        let Some(found) = all.into_iter().find(|found| found.id == id(&vault, name)) else {
            panic!("{name} found");
        };
        found
    };
    for (name, text) in texts {
        let found = row(name);
        let cancel = AtomicBool::new(true);
        assert_eq!(
            excerpt(&index.snapshot(), &found, &query, text, &cancel),
            None,
            "{name}"
        );
    }
}

#[test]
fn leading_indentation_is_trimmed_at_a_line_start() {
    assert_eq!(
        shown("intro\n    \tplan here", "plan"),
        ("plan here".into(), vec![(0, 4)])
    );
    assert_eq!(shown("  \tplan", "plan"), ("plan".into(), vec![(0, 4)]));
    let (text, highlights) = shown(&format!("a{}plan", " ".repeat(70)), "plan");
    assert_eq!(text, format!("{}plan", " ".repeat(60)), "mid-line: kept");
    assert_eq!(highlights, [(60, 64)]);
}

#[test]
fn a_word_ending_inside_a_grapheme_is_lit_to_the_grapheme_end() {
    // One trimmed space shifts the word, so a wrong end would land inside the modifier.
    assert_eq!(
        shown(" plan🏻 x", "plan"),
        ("plan🏻 x".into(), vec![(0, 8)])
    );
}

#[test]
fn a_text_without_a_matching_word_gives_none() {
    let (_vault, index) = with_texts(&[("n.md", Some("a plan"))]);
    let found = found(&index, &NoteQuery::new("plan", None, &[]));
    let other = NoteQuery::new("zzz", None, &[]);
    assert_eq!(
        excerpt(
            &index.snapshot(),
            &found,
            &other,
            "a plan",
            &AtomicBool::new(false)
        ),
        None
    );
}

fn text() -> impl Strategy<Value = String> {
    let parts: Vec<&str> = PARTS
        .iter()
        .copied()
        .chain(["b", "B", "plan", "Pla", "z"])
        .collect();
    prop::collection::vec(prop::sample::select(parts), 0..40).prop_map(|parts| parts.concat())
}

fn query() -> impl Strategy<Value = String> {
    let parts: Vec<&str> = PARTS
        .iter()
        .copied()
        .filter(|part| part.chars().count() <= 20)
        .chain(["b", "pl", "z "])
        .collect();
    prop::collection::vec(prop::sample::select(parts), 1..4).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn the_pre_check_never_changes_the_answer(text in text(), query in query()) {
        let query = NoteQuery::new(&query, None, &[]);
        let checked = Terms::new(&query);
        // Every ASCII byte may start a term: the pre-check skips nothing.
        let unchecked = Terms { starts: [true; 128], ..Terms::new(&query) };
        let cancel = AtomicBool::new(false);
        prop_assert_eq!(
            around_match(&text, &checked, &cancel),
            around_match(&text, &unchecked, &cancel)
        );
    }
}
