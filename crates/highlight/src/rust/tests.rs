use std::ops::Range;

use inkra_text::ByteOffset;

use super::lex;
use crate::{Language, TokenClass, highlight};

const KEYWORDS: [&str; 35] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

fn keywords(ranges: &[(usize, usize)]) -> Vec<(Range<ByteOffset>, TokenClass)> {
    ranges
        .iter()
        .map(|&(start, end)| {
            (
                ByteOffset::new(start)..ByteOffset::new(end),
                TokenClass::Keyword,
            )
        })
        .collect()
}

fn lexed(line: &str, start: usize) -> Vec<(Range<ByteOffset>, TokenClass)> {
    let mut out = Vec::new();
    lex(line, ByteOffset::new(start), &mut out);
    out
}

#[test]
fn keywords_are_tokens_at_the_line_start_offset() {
    assert_eq!(lexed("pub fn x", 10), keywords(&[(10, 13), (14, 16)]));
}

#[test]
fn a_keyword_inside_a_longer_word_is_no_token() {
    assert_eq!(
        lexed("fnord let_x 9let mut éfn fnö", 0),
        keywords(&[(17, 20)])
    );
}

#[test]
fn every_keyword_alone_is_one_token() {
    for keyword in KEYWORDS {
        assert_eq!(
            lexed(keyword, 0),
            keywords(&[(0, keyword.len())]),
            "{keyword}"
        );
    }
}

#[test]
fn constants_and_contextual_words_are_no_keywords() {
    assert_eq!(lexed("Self true false union macro_rules", 0), []);
}

#[test]
fn highlight_places_each_line_at_its_own_offset() {
    let mut out = Vec::new();
    highlight(
        Language::Rust,
        [(ByteOffset::new(0), "let"), (ByteOffset::new(20), "  mut")].into_iter(),
        &mut out,
    );
    assert_eq!(out, keywords(&[(0, 3), (22, 25)]));
}

#[test]
fn highlight_appends_to_what_out_holds() {
    let mut out = keywords(&[(0, 1)]);
    highlight(
        Language::Rust,
        [(ByteOffset::new(5), "fn")].into_iter(),
        &mut out,
    );
    assert_eq!(out, keywords(&[(0, 1), (5, 7)]));
}
