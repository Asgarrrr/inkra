use inkra_text::ByteOffset;

use crate::TokenClass::{self, Comment, Keyword, String, Type};
use crate::{Language, highlight};

type Token = (usize, usize, TokenClass);

/// The tokens of `text`, each line at its offset in `text`.
fn tokens(text: &str) -> Vec<Token> {
    let mut start = 0;
    let lines = text.split('\n').map(|line| {
        let at = start;
        start += line.len() + 1;
        (ByteOffset::new(at), line)
    });
    let mut out = Vec::new();
    highlight(Language::Rust, lines, &mut out);
    out.into_iter()
        .map(|(range, class)| (range.start.get(), range.end.get(), class))
        .collect()
}

fn check(cases: &[(&str, &[Token])]) {
    for &(text, expected) in cases {
        assert_eq!(tokens(text), expected, "{text:?}");
    }
}

#[test]
fn a_line_comment_runs_to_the_line_end() {
    check(&[
        ("fn x // let \"a", &[(0, 2, Keyword), (5, 14, Comment)]),
        (
            "/// a\n//! b\nfn",
            &[(0, 5, Comment), (6, 11, Comment), (12, 14, Keyword)],
        ),
    ]);
}

#[test]
fn a_block_comment_nests_and_hides_everything_inside() {
    check(&[
        ("/**/ fn", &[(0, 4, Comment), (5, 7, Keyword)]),
        ("/***/ fn", &[(0, 5, Comment), (6, 8, Keyword)]),
        ("/*/ fn", &[(0, 6, Comment)]),
        ("1 / 2 fn", &[(6, 8, Keyword)]),
        ("/* /* */ x */ fn", &[(0, 13, Comment), (14, 16, Keyword)]),
        (
            "/* fn \"x\" 'a // */ fn",
            &[(0, 18, Comment), (19, 21, Keyword)],
        ),
    ]);
}

#[test]
fn a_block_comment_carries_its_depth_across_lines() {
    check(&[
        (
            "/* /*\n*/ fn\n\n*/ fn",
            &[
                (0, 5, Comment),
                (6, 11, Comment),
                (13, 15, Comment),
                (16, 18, Keyword),
            ],
        ),
        (
            "fn /*\nlet\n*/ let",
            &[
                (0, 2, Keyword),
                (3, 5, Comment),
                (6, 9, Comment),
                (10, 12, Comment),
                (13, 16, Keyword),
            ],
        ),
    ]);
}

#[test]
fn a_string_takes_escapes_and_its_prefix() {
    check(&[
        ("\"\\\"\" fn", &[(0, 4, String), (5, 7, Keyword)]),
        ("\"a\\\"b\" fn", &[(0, 6, String), (7, 9, Keyword)]),
        ("\"a\\\\\" fn", &[(0, 5, String), (6, 8, Keyword)]),
        ("\"é\\中\" fn", &[(0, 8, String), (9, 11, Keyword)]),
        (
            "b\"x\" c\"y\" fn",
            &[(0, 4, String), (5, 9, String), (10, 12, Keyword)],
        ),
        ("ab\"x\"", &[(2, 5, String)]),
        ("\"fn", &[(0, 3, String)]),
    ]);
}

#[test]
fn a_string_spans_lines_one_token_per_line() {
    check(&[
        (
            "let s = \"a\\\nfn\" fn",
            &[
                (0, 3, Keyword),
                (8, 11, String),
                (12, 15, String),
                (16, 18, Keyword),
            ],
        ),
        (
            "\"a\n\nb\" fn",
            &[(0, 2, String), (4, 6, String), (7, 9, Keyword)],
        ),
    ]);
}

#[test]
fn a_raw_string_has_no_escapes_and_closes_on_its_hashes() {
    check(&[
        ("r\"a\\\" fn", &[(0, 5, String), (6, 8, Keyword)]),
        ("r#\"a\"b\"# fn", &[(0, 8, String), (9, 11, Keyword)]),
        ("br##\"x\"#\"## fn", &[(0, 11, String), (12, 14, Keyword)]),
        ("cr\"x\" fn", &[(0, 5, String), (6, 8, Keyword)]),
        (
            "r#\"a\n\"# fn",
            &[(0, 4, String), (5, 7, String), (8, 10, Keyword)],
        ),
        ("xr\"a\"", &[(2, 5, String)]),
    ]);
}

#[test]
fn a_raw_string_hash_count_saturates() {
    let hashes = "#".repeat(256);
    let text = format!("r{hashes}\"x\"{hashes}");

    assert_eq!(tokens(&text), [(0, 515, String)]);
}

#[test]
fn a_raw_identifier_is_never_a_keyword() {
    check(&[
        ("r#fn r#let fn", &[(11, 13, Keyword)]),
        ("r fn x#fn", &[(2, 4, Keyword), (7, 9, Keyword)]),
    ]);
}

#[test]
fn a_quote_opens_a_char_literal_or_a_lifetime() {
    check(&[
        ("'a' fn", &[(0, 3, String), (4, 6, Keyword)]),
        (
            "'中' ' ' '🦀'",
            &[(0, 5, String), (6, 9, String), (10, 16, String)],
        ),
        ("'\"' fn", &[(0, 3, String), (4, 6, Keyword)]),
        (
            "'\\n' '\\'' '\\\\'",
            &[(0, 4, String), (5, 9, String), (10, 14, String)],
        ),
        ("'\\u{1F980}' fn", &[(0, 11, String), (12, 14, Keyword)]),
        ("'\\u{1F980 fn", &[(0, 12, String)]),
        ("b'x' fn", &[(0, 4, String), (5, 7, Keyword)]),
        ("c'x' fn", &[(1, 4, String), (5, 7, Keyword)]),
        ("&'a str", &[(1, 3, Type)]),
        ("'static fn", &[(0, 7, Type), (8, 10, Keyword)]),
        ("'r#fn fn", &[(0, 5, Type), (6, 8, Keyword)]),
        ("'é", &[(0, 3, Type)]),
        ("'1 fn", &[(0, 5, String)]),
        ("'", &[(0, 1, String)]),
    ]);
}

#[test]
fn a_char_literal_or_lifetime_never_spans_lines() {
    check(&[
        ("'\\\nfn", &[(0, 2, String), (3, 5, Keyword)]),
        ("'a\nfn", &[(0, 2, Type), (3, 5, Keyword)]),
    ]);
}
