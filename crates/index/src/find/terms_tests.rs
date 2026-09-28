use proptest::prelude::*;

use super::terms::Terms;
use crate::NoteQuery;
use crate::test_support::PARTS;
use crate::words::{fold, word_spans};

/// `PARTS`, plus ASCII chars whose orbit holds a non-ASCII one, and kana with a voicing mark.
fn parts() -> Vec<&'static str> {
    PARTS
        .iter()
        .copied()
        .chain([
            "K",
            "S",
            "z",
            "Z",
            "\u{17F}",
            "\u{212A}",
            "か\u{3099}",
            "が",
            "\u{3099}",
        ])
        .collect()
}

fn text(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(parts()), 0..len).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn the_first_byte_skip_never_drops_a_matching_term(text in text(12), query in text(4)) {
        let query = NoteQuery::new(&query, None, &[]);
        let checked = Terms::new(&query);
        let unchecked = Terms { starts: [true; 128], ..Terms::new(&query) };
        for (_, word) in word_spans(&text) {
            prop_assert_eq!(
                checked.matching(word).collect::<Vec<_>>(),
                unchecked.matching(word).collect::<Vec<_>>(),
                "{:?} for {:?}", word, query
            );
        }
    }
}

#[test]
fn the_first_byte_skip_is_exact_for_every_ascii_char() {
    let words: Vec<String> = (0..128_u8)
        .flat_map(|byte| {
            ["", "\u{301}", "\u{307}", "\u{34F}\u{301}", "\u{3099}x"]
                .map(|suffix| format!("{}{suffix}", char::from(byte)))
        })
        .collect();
    let mut terms: Vec<String> = words
        .iter()
        .flat_map(|word| {
            let folded = fold(word).into_owned();
            let ends: Vec<usize> = folded
                .char_indices()
                .map(|(at, c)| at + c.len_utf8())
                .collect();
            ends.into_iter().map(move |end| folded[..end].to_owned())
        })
        .collect();
    terms.sort_unstable();
    terms.dedup();
    for term in &terms {
        let query = NoteQuery::new(term, None, &[]);
        let checked = Terms::new(&query);
        let unchecked = Terms {
            starts: [true; 128],
            ..Terms::new(&query)
        };
        for word in &words {
            assert_eq!(
                checked.matching(word).collect::<Vec<_>>(),
                unchecked.matching(word).collect::<Vec<_>>(),
                "{word:?} for {term:?}"
            );
        }
    }
}
