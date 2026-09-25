use std::sync::atomic::AtomicBool;

use inkra_text::ByteOffset;

use crate::test_support::corpus;
use crate::{HITS_PER_NOTE, Hit, Searched, TextQuery, search};

/// The text of each match, all flags given.
fn found<'a>(query: &str, sensitive: bool, word: bool, text: &'a str) -> Vec<&'a str> {
    let Some(query) = TextQuery::new(query, sensitive, word) else {
        panic!("{query:?} is a query");
    };
    query
        .find_in(text, usize::MAX)
        .into_iter()
        .map(|range| &text[range.start.0..range.end.0])
        .collect()
}

#[test]
fn an_empty_or_blank_query_is_none() {
    for text in ["", " ", "\t\n", "\u{3000}"] {
        assert!(TextQuery::new(text, false, false).is_none(), "{text:?}");
    }
}

#[test]
fn a_query_blank_up_to_the_cut_is_none() {
    assert!(TextQuery::new(&(" ".repeat(1_000) + "a"), false, false).is_none());
}

#[test]
fn case_folds_unless_sensitive() {
    assert_eq!(found("été", false, false, "L'Été"), ["Été"]);
    assert_eq!(found("été", true, false, "L'Été"), Vec::<&str>::new());
    assert_eq!(found("k", false, false, "\u{212A}"), ["\u{212A}"]);
}

#[test]
fn whole_word_needs_a_boundary_at_each_word_edge() {
    assert_eq!(found("été", false, true, "l'été, "), ["été"]);
    assert_eq!(found("été", false, true, "des étés"), Vec::<&str>::new());
    assert_eq!(found("été", false, true, "bété"), Vec::<&str>::new());
    assert_eq!(found("été", false, false, "des étés"), ["été"]);
}

#[test]
fn a_punctuation_edge_adds_no_boundary() {
    assert_eq!(found("-plan", false, true, "x-plan y"), ["-plan"]);
    assert_eq!(
        found("-plan", false, true, "x-planning"),
        Vec::<&str>::new()
    );
    assert_eq!(found("plan!", false, true, "a plan!x"), ["plan!"]);
    assert_eq!(found("plan!", false, true, "aplan!"), Vec::<&str>::new());
    assert_eq!(found("-plan", false, true, "--plan"), ["-plan"]);
}

#[test]
fn a_query_keeps_its_spaces() {
    assert_eq!(found("a ", false, false, "ab a b"), ["a "]);
}

#[test]
fn a_word_char_is_regex_s_not_rust_s() {
    // `½` is alphanumeric for Rust but not a regex word char: no boundary is required.
    assert_eq!(found("½", false, true, "1½ "), ["½"]);
}

#[test]
fn the_query_is_nfc_and_the_text_is_not_normalized() {
    assert_eq!(found("e\u{301}te\u{301}", false, false, "été"), ["été"]);
    assert_eq!(
        found("été", false, false, "e\u{301}te\u{301}"),
        Vec::<&str>::new()
    );
}

#[test]
fn find_in_stops_at_the_limit_and_returns_byte_offsets() {
    let Some(query) = TextQuery::new("b", false, false) else {
        panic!("a query");
    };
    assert_eq!(
        query.find_in("ébbb", 2),
        [ByteOffset(2)..ByteOffset(3), ByteOffset(3)..ByteOffset(4)]
    );
    assert!(query.find_in("ébbb", 0).is_empty());
}

fn query(text: &str) -> TextQuery {
    let Some(query) = TextQuery::new(text, false, false) else {
        panic!("{text:?} is a query");
    };
    query
}

fn run(texts: &[Option<&str>], text: &str, limit: usize) -> Searched {
    let (_, corpus) = corpus(texts);
    search(
        &corpus,
        0..corpus.len(),
        &query(text),
        &AtomicBool::new(false),
        limit,
    )
}

#[test]
fn hits_come_in_vault_order_and_skip_notes_not_loaded() {
    let (ids, corpus) = corpus(&[Some("x plan"), None, Some("plan plan")]);
    let found = search(&corpus, 0..3, &query("plan"), &AtomicBool::new(false), 10);
    let at: Vec<_> = found
        .hits
        .iter()
        .map(|hit| (hit.note, hit.range.start.0))
        .collect();
    assert_eq!(at, [(ids[0], 2), (ids[2], 0), (ids[2], 5)]);
    assert!(!found.limit_reached);
}

#[test]
fn a_note_gives_at_most_100_hits() {
    let many = "a".repeat(150);
    let found = run(&[Some(&many), Some("a")], "a", 1_000);
    assert_eq!(HITS_PER_NOTE, 100);
    assert_eq!(found.hits.len(), HITS_PER_NOTE + 1);
    assert!(!found.limit_reached);
}

#[test]
fn the_search_stops_at_the_limit() {
    let found = run(&[Some("aaa"), Some("aa")], "a", 4);
    assert_eq!(found.hits.len(), 4);
    assert!(found.limit_reached);
    let exact = run(&[Some("aa"), Some("aa")], "a", 4);
    assert!(
        exact.limit_reached,
        "exactly `limit` hits reaches the limit"
    );
    assert!(run(&[Some("a")], "a", 0).limit_reached);
}

#[test]
fn a_set_flag_returns_nothing() {
    let (_, corpus) = corpus(&[Some("a"), Some("a")]);
    let found = search(&corpus, 0..2, &query("a"), &AtomicBool::new(true), 10);
    assert_eq!(found, Searched::default());
    let none = search(&corpus, 0..2, &query("a"), &AtomicBool::new(true), 0);
    assert_eq!(none, Searched::default(), "even at a limit of 0");
}

#[test]
fn a_range_searches_only_its_notes() {
    let (ids, corpus) = corpus(&[Some("a"), Some("a"), Some("a")]);
    let found = search(&corpus, 1..2, &query("a"), &AtomicBool::new(false), 10);
    let notes: Vec<_> = found.hits.iter().map(|hit| hit.note).collect();
    assert_eq!(notes, [ids[1]]);
}

fn context_of(text: &str, query_text: &str) -> (String, String) {
    let found = run(&[Some(text)], query_text, 1);
    let Some(Hit {
        context,
        context_match,
        ..
    }) = found.hits.into_iter().next()
    else {
        panic!("{query_text:?} in {text:?}");
    };
    let matched = context[context_match].to_owned();
    (context, matched)
}

#[test]
fn context_keeps_60_bytes_before_and_100_after_within_the_line() {
    let text = format!("{}HIT{}", "b".repeat(70), "a".repeat(120));
    let (context, matched) = context_of(&text, "hit");
    assert_eq!(matched, "HIT");
    assert_eq!(context, format!("{}HIT{}", "b".repeat(60), "a".repeat(100)));

    let (context, _) = context_of("first\nsay hit here\nlast", "hit");
    assert_eq!(context, "say hit here");
}

#[test]
fn context_cuts_on_char_boundaries() {
    // 61 bytes before the hit start mid-`é`; 100 after end mid-`é`.
    let text = format!("é{}HIT{}é", "b".repeat(59), "a".repeat(99));
    let (context, matched) = context_of(&text, "hit");
    assert_eq!(matched, "HIT");
    assert_eq!(context, format!("{}HIT{}", "b".repeat(59), "a".repeat(99)));
}

#[test]
fn context_stops_at_a_newline_on_the_first_or_last_byte_of_its_window() {
    // The newlines sit 60 bytes before the hit and 99 after it.
    let text = format!("x\n{}HIT{}\ny", "b".repeat(59), "a".repeat(99));
    let (context, matched) = context_of(&text, "hit");
    assert_eq!(matched, "HIT");
    assert_eq!(context, format!("{}HIT{}", "b".repeat(59), "a".repeat(99)));
}

#[test]
fn context_is_never_shorter_than_the_hit() {
    let long = "x".repeat(300);
    let (context, matched) = context_of(&long, &long);
    assert_eq!(context, long);
    assert_eq!(matched, long);
}

#[test]
fn merge_concatenates_in_range_order_and_truncates() {
    let (_, corpus) = corpus(&[Some("aa"), Some("aa"), Some("aa")]);
    let q = query("a");
    let cancel = AtomicBool::new(false);
    let parts: Vec<Searched> = corpus
        .chunks(3)
        .into_iter()
        .map(|range| search(&corpus, range, &q, &cancel, 5))
        .collect();
    assert_eq!(
        Searched::merge(parts, 5),
        search(&corpus, 0..3, &q, &cancel, 5)
    );
    assert_eq!(Searched::merge(Vec::new(), 5), Searched::default());
}

#[test]
fn a_set_flag_returns_nothing_even_with_no_loaded_note_at_a_limit_of_0() {
    let (_, corpus) = corpus(&[None, None]);
    let cancel = AtomicBool::new(true);
    assert_eq!(
        search(&corpus, 0..2, &query("a"), &cancel, 0),
        Searched::default(),
        "no loaded note"
    );
    assert_eq!(
        search(&corpus, 1..1, &query("a"), &cancel, 0),
        Searched::default(),
        "an empty range"
    );
}

#[test]
fn a_huge_query_is_cut_to_its_first_1000_chars() {
    let long = "aé".repeat(50_000);
    let cut = "aé".repeat(500);
    let expected = found(&cut, false, false, &long);
    assert_eq!(expected.len(), 100);
    assert_eq!(found(&long, false, false, &long), expected);
}

#[test]
fn each_form_of_the_query_decides_its_own_word_edges() {
    // `<` + U+0338 composes to `≮`: the mark is a word char, `≮` is not.
    assert_eq!(
        found("<\u{338}", true, true, "<\u{338}x"),
        Vec::<&str>::new()
    );
    assert_eq!(found("<\u{338}", true, true, "\u{226E}x"), ["\u{226E}"]);
}

#[test]
fn both_forms_of_a_long_query_are_cut_at_the_same_typed_char() {
    let query = "e\u{301}".repeat(600);
    let nfd = "e\u{301}".repeat(600);
    let nfc = "\u{e9}".repeat(600);
    assert_eq!(found(&query, true, false, &nfd), [&nfd[..1_500]]);
    assert_eq!(found(&query, true, false, &nfc), [&nfc[..1_000]]);
}

#[test]
fn a_query_pasted_from_a_note_finds_it() {
    // NFC maps these singletons to another char, so the query no longer equals the text.
    for text in ["\u{212A}", "\u{212B}", "\u{2126}", "\u{F91D}"] {
        assert_eq!(
            found(text, true, false, text),
            [text],
            "{text:?}, sensitive"
        );
        assert_eq!(
            found(text, false, false, text),
            [text],
            "{text:?}, insensitive"
        );
    }
}

#[test]
fn a_whole_word_match_can_overlap_a_rejected_one() {
    // `xé` rejects the first `é é`; the valid one starts inside it, at byte 4.
    let Some(query) = TextQuery::new("é é", false, true) else {
        panic!("a query");
    };
    assert_eq!(
        query.find_in("xé é é", usize::MAX),
        [ByteOffset(4)..ByteOffset(9)]
    );
}

#[test]
fn whole_word_checks_non_ascii_edges_over_greek_and_cyrillic() {
    assert_eq!(found("σ", false, true, "σχέδιο σ, ς ασ Σ"), ["σ", "ς", "Σ"]);
    assert_eq!(found("σ", true, true, "σχέδιο σ, ς ασ Σ"), ["σ"]);
    assert_eq!(found("е", false, true, "где е, Е нет ё"), ["е", "Е"]);
    assert_eq!(found("ет", false, true, "нет ет ета ет."), ["ет", "ет"]);
    assert_eq!(found("ά", false, true, "ά άλλο Ά"), ["ά", "Ά"]);
}

#[test]
fn whole_word_checks_an_ascii_edge_beside_a_non_ascii_one() {
    assert_eq!(found("café", false, true, "xcafé cafés Café."), ["Café"]);
    assert_eq!(found("été", false, true, "été2 _été été"), ["été"]);
}

#[test]
fn whole_word_stops_at_the_limit_after_a_rejected_match() {
    let Some(query) = TextQuery::new("é", false, true) else {
        panic!("a query");
    };
    assert_eq!(query.find_in("xé é é", 1), [ByteOffset(4)..ByteOffset(6)]);
    assert!(query.find_in("xé é", 0).is_empty());
}
