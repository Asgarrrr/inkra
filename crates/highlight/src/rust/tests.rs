use std::ops::Range;

use inkra_text::ByteOffset;

use super::lex;
use crate::TokenClass::{self, Comment, Constant, Function, Keyword, Property, String, Type};
use crate::state::LineState;
use crate::{Language, highlight};

const KEYWORDS: [&str; 35] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

pub(super) type Token = (usize, usize, TokenClass);

/// The tokens of `text`, each line at its offset in `text`.
pub(super) fn tokens(text: &str) -> Vec<Token> {
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

pub(super) fn check(cases: &[(&str, &[Token])]) {
    for &(text, expected) in cases {
        assert_eq!(tokens(text), expected, "{text:?}");
    }
}

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
    lex(LineState::default(), line, ByteOffset::new(start), &mut out);
    out
}

#[test]
fn keywords_are_tokens_at_the_line_start_offset() {
    assert_eq!(lexed("pub mod x", 10), keywords(&[(10, 13), (14, 17)]));
}

#[test]
fn a_keyword_inside_a_longer_word_is_no_token() {
    check(&[(
        "fnord let_x 9let mut éfn fnö",
        &[(12, 16, Constant), (17, 20, Keyword)],
    )]);
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
    check(&[(
        "Self true false union macro_rules",
        &[(0, 4, Type), (5, 9, Constant), (10, 15, Constant)],
    )]);
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

#[test]
fn a_keyword_wins_over_every_context() {
    check(&[
        ("x.await", &[(2, 7, Keyword)]),
        ("fn self()", &[(0, 2, Keyword), (3, 7, Keyword)]),
        ("struct if!", &[(0, 6, Keyword), (7, 9, Keyword)]),
    ]);
}

#[test]
fn after_a_dot_a_call_is_a_function_and_else_a_property() {
    check(&[
        ("p.x", &[(2, 3, Property)]),
        ("v.len ()", &[(2, 5, Function)]),
        (
            "p.MAX p.X p.i32",
            &[(2, 5, Property), (8, 9, Property), (12, 15, Property)],
        ),
        ("p.f!()", &[(2, 3, Property)]),
        ("p. /* c */ x", &[(3, 10, Comment), (11, 12, Property)]),
        ("p.\nx", &[(3, 4, Property)]),
        ("a..x a...x", &[]),
    ]);
}

#[test]
fn a_name_before_a_bang_is_a_macro_with_its_bang() {
    check(&[
        ("println!(\"a\")", &[(0, 8, Function), (9, 12, String)]),
        ("Vec! MAX!", &[(0, 4, Function), (5, 9, Function)]),
        ("x != y x!=y", &[]),
    ]);
}

#[test]
fn after_fn_a_name_is_a_function() {
    check(&[
        ("fn main", &[(0, 2, Keyword), (3, 7, Function)]),
        (
            "fn Main fn MAX",
            &[
                (0, 2, Keyword),
                (3, 7, Function),
                (8, 10, Keyword),
                (11, 14, Function),
            ],
        ),
        (
            "fn /* c */ main",
            &[(0, 2, Keyword), (3, 10, Comment), (11, 15, Function)],
        ),
        ("fn\nmain", &[(0, 2, Keyword), (3, 7, Function)]),
        ("fn r#main", &[(0, 2, Keyword), (3, 9, Function)]),
    ]);
}

#[test]
fn after_a_type_keyword_a_name_is_a_type() {
    for keyword in ["struct", "enum", "trait", "type", "impl"] {
        let len = keyword.len();
        check(&[(
            &format!("{keyword} t"),
            &[(0, len, Keyword), (len + 1, len + 2, Type)],
        )]);
    }
    check(&[
        ("struct MAX", &[(0, 6, Keyword), (7, 10, Type)]),
        ("struct f()", &[(0, 6, Keyword), (7, 8, Type)]),
    ]);
}

#[test]
fn booleans_and_all_caps_names_are_constants() {
    check(&[
        ("true false", &[(0, 4, Constant), (5, 10, Constant)]),
        (
            "MAX AB A1_B MAX()",
            &[
                (0, 3, Constant),
                (4, 6, Constant),
                (7, 11, Constant),
                (12, 15, Constant),
            ],
        ),
        ("N Ab _A", &[(0, 1, Type), (2, 4, Type)]),
    ]);
}

#[test]
fn a_capitalised_name_is_a_type() {
    check(&[(
        "Self T Some(x)",
        &[(0, 4, Type), (5, 6, Type), (7, 11, Type)],
    )]);
}

#[test]
fn every_builtin_type_alone_is_a_type() {
    for name in [
        "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32",
        "i64", "i128", "isize", "f32", "f64",
    ] {
        check(&[(name, &[(0, name.len(), Type)])]);
    }
    check(&[("u8(x)", &[(0, 2, Type)])]);
}

#[test]
fn a_name_before_a_paren_on_its_line_is_a_function() {
    check(&[
        ("foo(x)", &[(0, 3, Function)]),
        ("foo (x)", &[(0, 3, Function)]),
        ("foo bar\n(x)", &[]),
    ]);
}

#[test]
fn every_other_token_resets_the_look_behind() {
    check(&[
        ("fn \"s\" x", &[(0, 2, Keyword), (3, 6, String)]),
        ("fn 'a x", &[(0, 2, Keyword), (3, 5, Type)]),
        ("fn 1 x", &[(0, 2, Keyword), (3, 4, Constant)]),
        ("fn < x", &[(0, 2, Keyword)]),
        ("fn y x", &[(0, 2, Keyword), (3, 4, Function)]),
        ("fn const x", &[(0, 2, Keyword), (3, 8, Keyword)]),
        ("fn r#fn x", &[(0, 2, Keyword), (3, 7, Function)]),
        (". \"s\" x", &[(2, 5, String)]),
    ]);
}
