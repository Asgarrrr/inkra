use proptest::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

use super::label::word_starts;
use crate::words::{NoteQuery, fold, word_spans};

/// Ranges as pairs, so a one-range expectation needs no one-range `Vec`, which clippy rejects.
fn lit(query: &str, text: &str) -> Option<Vec<(usize, usize)>> {
    let lit = word_starts(&NoteQuery::new(query, None, &[]), text)?;
    Some(
        lit.into_iter()
            .map(|range| (range.start, range.end))
            .collect(),
    )
}

#[test]
fn every_term_must_start_a_word_in_any_order() {
    assert_eq!(lit("new no", "New note"), Some(vec![(0, 3), (4, 6)]));
    assert_eq!(lit("note new", "New note"), Some(vec![(0, 3), (4, 8)]));
    assert_eq!(lit("ote", "New note"), None);
    assert_eq!(lit("new x", "New note"), None);
}

#[test]
fn one_word_may_serve_two_terms() {
    assert_eq!(lit("new n", "New folder"), Some(vec![(0, 3)]));
    assert_eq!(lit("n ne", "New note"), Some(vec![(0, 2), (4, 5)]));
}

#[test]
fn folds_case_and_lights_whole_graphemes() {
    assert_eq!(lit("SAVE", "Save"), Some(vec![(0, 4)]));
    assert_eq!(lit("ete", "\u{c9}t\u{e9}"), Some(vec![(0, 5)]));
    assert_eq!(lit("et", "\u{c9}t\u{e9}"), Some(vec![(0, 3)]));
}

#[test]
fn an_empty_query_matches_with_nothing_lit() {
    assert_eq!(lit("", "Quit"), Some(vec![]));
    assert_eq!(lit("  ", "Quit"), Some(vec![]));
}

fn text() -> impl Strategy<Value = String> {
    "[a-zA-Z\u{e9}\u{c9}\u{301} -]{0,24}"
}

proptest! {
    #[test]
    fn a_text_matches_itself_at_every_word(text in text()) {
        let found = lit(&text, &text);
        prop_assert!(found.is_some());
        let found = found.unwrap_or_default();
        // A word that folds to nothing, a lone combining mark, is never a term.
        for (at, _) in word_spans(&text).filter(|(_, word)| !fold(word).is_empty()) {
            prop_assert!(found.iter().any(|&(start, end)| start <= at && at < end), "{at} unlit in {found:?}");
        }
    }

    #[test]
    fn term_order_and_case_change_nothing(a in "[a-z\u{e9}]{1,4}", b in "[a-z\u{e9}]{1,4}", text in text()) {
        let ab = lit(&format!("{a} {b}"), &text);
        prop_assert_eq!(&ab, &lit(&format!("{b} {a}"), &text));
        prop_assert_eq!(&ab, &lit(&format!("{} {}", a.to_uppercase(), b.to_uppercase()), &text));
    }

    #[test]
    fn a_term_more_never_matches_more(a in "[a-z]{1,4}", b in "[a-z]{1,4}", text in text()) {
        if lit(&a, &text).is_none() {
            let both = format!("{a} {b}");
            prop_assert!(lit(&both, &text).is_none());
        }
    }

    #[test]
    fn ranges_ascend_apart_on_word_starts(query in "[a-z\u{e9} ]{0,8}", text in text()) {
        let starts: Vec<usize> = word_spans(&text).map(|(at, _)| at).collect();
        let graphemes: Vec<usize> = text.grapheme_indices(true).map(|(at, _)| at).collect();
        let mut end = None;
        for (start, stop) in lit(&query, &text).into_iter().flatten() {
            prop_assert!(start < stop && stop <= text.len());
            prop_assert!(text.is_char_boundary(start) && text.is_char_boundary(stop));
            prop_assert!(starts.iter().any(|&at| start <= at && at < stop));
            prop_assert!(graphemes.contains(&start));
            prop_assert!(end.is_none_or(|end| end < start));
            end = Some(stop);
        }
    }
}
