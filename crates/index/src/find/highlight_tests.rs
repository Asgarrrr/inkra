use std::ops::Range;

use super::highlight::{boundaries, prefix};
use super::tests::{find, with_names};
use crate::NoteMatch;

fn lit(found: &NoteMatch) -> Vec<&str> {
    found
        .name_highlights
        .iter()
        .map(|range| &found.path[range.clone()])
        .collect()
}

/// The highlights as `(start, end)`, which clippy accepts where a lone `Range` is flagged.
fn spans(found: &NoteMatch) -> Vec<(usize, usize)> {
    found
        .name_highlights
        .iter()
        .map(|range| (range.start, range.end))
        .collect()
}

fn only(files: &[&str], text: &str) -> NoteMatch {
    let (_vault, index) = with_names(files);
    let mut found = find(&index, text);
    assert_eq!(found.len(), 1, "one match for {text:?}");
    found.remove(0)
}

#[test]
fn highlights_cover_the_matched_prefix_in_whole_graphemes() {
    let planning = only(&["Planning.md"], "pla");
    assert_eq!(spans(&planning), [(0, 3)]);
    assert_eq!(lit(&planning), ["Pla"]);

    // `q\u{301}` has no precomposed form, so NFC keeps the mark: one grapheme of 3 bytes.
    let marked = only(&["q\u{301}uiz.md"], "q");
    assert_eq!(spans(&marked), [(0, 3)]);
    assert_eq!(spans(&only(&["e\u{301}te.md"], "e")), [(0, "é".len())]);

    let emoji = only(&["👩\u{200d}💻 Plan.md"], "pla");
    assert_eq!(lit(&emoji), ["Pla"]);
    let keycap = only(&["1\u{fe0f}\u{20e3}.md"], "1");
    assert_eq!(spans(&keycap), [(0, "1\u{fe0f}\u{20e3}".len())]);

    assert_eq!(
        spans(&only(&["Planning.md"], "pla plan")),
        [(0, 4)],
        "two terms on one word merge"
    );
    // A word starting inside a grapheme, a modifier ending one, a prepended mark: whole graphemes.
    assert_eq!(spans(&only(&["🎉\u{301}x.md"], "x")), [(0, 7)]);
    assert_eq!(spans(&only(&["a\u{1F3FB}.md"], "a")), [(0, 5)]);
    assert_eq!(spans(&only(&["\u{600}1.md"], "1")), [(0, 3)]);
    assert_eq!(
        spans(&only(&["work/plan it.md"], "pl wo i")),
        [(0, 2), (5, 7), (10, 11)]
    );
}

#[test]
fn prefix_is_the_shortest_grapheme_run_folding_to_the_term() {
    let cases: [(&str, Range<usize>, &str, Range<usize>); 7] = [
        ("Planning", 0..8, "pla", 0..3),
        ("E\u{301}te", 0..5, "e", 0..3),
        ("한글", 0..6, "\u{1112}", 0..3),
        ("ab", 0..2, "zz", 0..2),
        ("a\u{200d}b", 0..5, "a", 0..4),
        ("🎉\u{301}x", 4..7, "x", 0..7),
        ("a/\u{301}b", 2..5, "b", 1..5),
    ];
    for (text, word, term, expected) in cases {
        assert_eq!(
            prefix(text, &boundaries(text), word.clone(), term),
            expected,
            "{text:?} {word:?} {term:?}"
        );
    }
}
