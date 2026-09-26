use std::iter;
use std::sync::atomic::AtomicBool;

use inkra_vault::EntryId;

use super::{NoteWords, TERM_MAX_CHARS, WORD_MAX_CHARS};
use crate::test_support::indexed;
use crate::{Lookup, NoteQuery, TermHit};

fn counts(text: &str) -> Vec<(String, u32)> {
    NoteWords::parse(text)
        .iter()
        .map(|(word, count)| (word.to_owned(), count))
        .collect()
}

fn terms(text: &str) -> Vec<String> {
    NoteQuery::new(text)
        .terms()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_note_s_words_are_folded_counted_and_sorted() {
    assert_eq!(
        counts("Été, l'été et ETE. Plan"),
        [
            ("et".to_owned(), 1),
            ("ete".to_owned(), 3),
            ("l".to_owned(), 1),
            ("plan".to_owned(), 1)
        ]
    );
    assert_eq!(counts(""), []);
    assert_eq!(
        counts("\u{301} ! \u{301}"),
        [],
        "a word folding to nothing is dropped"
    );
}

#[test]
fn a_long_word_is_stored_capped_and_still_found() {
    let long = "a".repeat(1_000_000);
    let found = counts(&format!("{long} {long}b"));
    assert_eq!(found, [("a".repeat(WORD_MAX_CHARS), 2)]);
}

#[test]
fn a_note_of_20_000_distinct_words_counts_each_once() {
    let words: Vec<String> = (0..20_000).map(|n| format!("w{n:015}")).collect();
    let expected: Vec<(String, u32)> = words.iter().map(|word| (word.clone(), 1)).collect();
    assert_eq!(counts(&words.join(" ")), expected);
}

#[test]
fn a_stored_word_keeps_at_most_65_folded_chars() {
    // 30 `한` fold to 90 jamo; the cut falls inside the 22nd syllable's jamo.
    let expected: String = "\u{1112}\u{1161}\u{11AB}"
        .repeat(30)
        .chars()
        .take(WORD_MAX_CHARS)
        .collect();
    assert_eq!(counts(&"한".repeat(30)), [(expected, 1)]);
}

#[test]
fn heap_bytes_counts_the_words_bounds_and_counts() {
    let words = NoteWords::parse("ab c ab");
    assert!(!words.is_empty());
    assert_eq!(
        words.heap_bytes(),
        "abc".len() + 3 * size_of::<usize>() + 2 * size_of::<u32>()
    );
    assert!(NoteWords::parse("—").is_empty());
    assert!(NoteWords::default().is_empty());
    assert_eq!(NoteWords::default(), NoteWords::parse(""));
}

#[test]
fn a_base64_image_splits_on_plus_and_slash_and_stays_capped() {
    const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // A fixed pseudo-random walk (an LCG's top 6 bits): a 1 MB paste of an image, not a period.
    let blob: String = iter::successors(Some(1_u32), |x| {
        Some(x.wrapping_mul(1_103_515_245).wrapping_add(12_345))
    })
    .take(1_000_000)
    .map(|x| char::from(BASE64[usize::from(x.to_be_bytes()[0] >> 2)]))
    .collect();
    let words = NoteWords::parse(&blob);
    assert!(
        words.iter().len() > 1_000,
        "`+` and `/` cut the blob into many words"
    );
    assert!(
        words
            .iter()
            .all(|(word, _)| word.chars().count() <= WORD_MAX_CHARS)
    );
    assert!(words.iter().all(|(word, _)| !word.contains(['+', '/'])));
}

#[test]
fn with_prefix_finds_the_run_of_words_starting_with_a_term() {
    let words = NoteWords::parse("pro projet projets apro prune");
    let found: Vec<(&str, u32)> = words.with_prefix("pro").collect();
    assert_eq!(found, [("pro", 1), ("projet", 1), ("projets", 1)]);
    assert_eq!(words.with_prefix("zz").count(), 0);
    assert_eq!(words.with_prefix("").count(), 5);
}

#[test]
fn a_query_splits_like_the_text_then_folds_and_drops_repeats() {
    assert_eq!(terms("Été  l'ÉTÉ plan"), ["ete", "l", "plan"]);
    assert_eq!(terms("aujourd'hui"), ["aujourd", "hui"]);
    assert_eq!(terms("plan PLAN plän"), ["plan"]);
}

#[test]
fn a_query_of_marks_is_empty() {
    assert!(NoteQuery::new("\u{301} \u{301}").is_empty());
    assert!(NoteQuery::new("  —  ").is_empty());
    assert!(NoteQuery::new("").is_empty());
    assert!(!NoteQuery::new("a").is_empty());
}

#[test]
fn a_query_keeps_its_first_64_typed_chars() {
    let typed = format!("{} tail", "a".repeat(64));
    assert_eq!(terms(&typed), ["a".repeat(64)]);
}

#[test]
fn a_query_cut_inside_its_last_word_drops_that_word() {
    let typed = format!("{} planning", "a".repeat(61));
    assert_eq!(typed.chars().count(), 70);
    assert_eq!(terms(&typed), ["a".repeat(61)]);
}

#[test]
fn a_query_cut_at_a_word_end_keeps_that_word() {
    let typed = format!("{} planning tail", "a".repeat(55));
    assert_eq!(terms(&typed), ["a".repeat(55), "planning".to_owned()]);
    let typed = format!("{} planning", "a".repeat(63));
    assert_eq!(terms(&typed), ["a".repeat(63)], "a word past the cut");
}

#[test]
fn a_query_of_one_word_cut_keeps_it_capped() {
    let typed = "planning".repeat(9);
    assert_eq!(terms(&typed), [typed[..64].to_owned()]);
}

#[test]
fn a_term_keeps_at_most_64_folded_chars() {
    // `한` folds to three jamo, so 40 of them would fold to 120 chars.
    let typed = "한".repeat(40);
    let found = terms(&typed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].chars().count(), TERM_MAX_CHARS);
    const {
        assert!(
            WORD_MAX_CHARS > TERM_MAX_CHARS,
            "a capped word never equals a term"
        );
    }
}

fn look(texts: &[Option<&str>], query: &str) -> (Vec<EntryId>, Lookup) {
    let (ids, index) = indexed(texts);
    let Some(found) = index
        .words()
        .lookup(&NoteQuery::new(query), &AtomicBool::new(false))
    else {
        panic!("not cancelled");
    };
    (ids, found)
}

#[test]
fn a_term_finds_every_word_it_starts_and_counts_them() {
    let (ids, found) = look(
        &[Some("projet projets Projet"), Some("pro"), Some("apro")],
        "pro",
    );
    assert_eq!(found.term_count(), 1);
    assert_eq!(
        found.get(0, ids[0]),
        Some(TermHit {
            count: 3,
            whole: false
        })
    );
    assert_eq!(
        found.get(0, ids[1]),
        Some(TermHit {
            count: 1,
            whole: true
        })
    );
    assert_eq!(
        found.get(0, ids[2]),
        None,
        "a term matches the start of a word only"
    );
}

#[test]
fn accents_and_case_fold_both_ways_in_lookup() {
    let (ids, found) = look(&[Some("Été"), Some("ete")], "ÉTÉ");
    let whole = Some(TermHit {
        count: 1,
        whole: true,
    });
    assert_eq!(found.get(0, ids[0]), whole);
    assert_eq!(found.get(0, ids[1]), whole);
}

#[test]
fn each_term_is_resolved_on_its_own() {
    let (ids, found) = look(&[Some("plan été"), Some("plan")], "plan ete");
    assert_eq!(found.term_count(), 2);
    assert!(found.get(0, ids[1]).is_some());
    assert_eq!(
        found.get(1, ids[0]),
        Some(TermHit {
            count: 1,
            whole: true
        })
    );
    assert_eq!(found.get(1, ids[1]), None);
    assert_eq!(
        found.get(2, ids[0]),
        None,
        "a term past the last is absent, not a panic"
    );
}

#[test]
fn a_note_not_loaded_is_absent() {
    let (ids, found) = look(&[None, Some("plan")], "plan");
    assert_eq!(found.get(0, ids[0]), None);
    assert!(found.get(0, ids[1]).is_some());
}

#[test]
fn a_set_cancel_returns_no_lookup() {
    let (_ids, index) = indexed(&[Some("plan")]);
    let found = index
        .words()
        .lookup(&NoteQuery::new("plan"), &AtomicBool::new(true));
    assert!(found.is_none());
}

#[test]
fn an_empty_query_looks_up_nothing() {
    let (_ids, found) = look(&[Some("plan")], "  ");
    assert_eq!(found.term_count(), 0);
}

#[test]
fn a_64_char_term_starts_a_longer_stored_word_and_equals_its_own_length() {
    let (long, exact) = ("a".repeat(80), "a".repeat(64));
    let (ids, found) = look(&[Some(&long), Some(&exact)], &exact);
    assert_eq!(
        found.get(0, ids[0]),
        Some(TermHit {
            count: 1,
            whole: false
        })
    );
    assert_eq!(
        found.get(0, ids[1]),
        Some(TermHit {
            count: 1,
            whole: true
        })
    );
}
