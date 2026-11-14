use inkra_text::ByteOffset;

use crate::TokenClass::{
    Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String, Type,
};
use crate::rust::lex;
use crate::rust::tests::{Token, check, tokens};
use crate::state::LineState;

fn state_after(line: &str) -> LineState {
    lex(
        LineState::default(),
        line,
        ByteOffset::new(0),
        &mut Vec::new(),
    )
}

#[test]
fn a_closed_nesting_leaves_the_default_state() {
    for line in [
        "()", "[]", "{}", "#[]", "#![]", "Vec<x>", "(Foo<x)", "{Foo<x}", ")", "]",
    ] {
        assert_eq!(state_after(line), LineState::default(), "{line:?}");
    }
}

#[test]
fn a_closer_pops_only_its_own_opener() {
    for line in ["(]", "[)", "{)", "(}", "#[)", "(>"] {
        assert_ne!(state_after(line), LineState::default(), "{line:?}");
    }
}

#[test]
fn the_nesting_stack_holds_sixteen_entries() {
    let cases: [(std::string::String, usize, &[Token]); 4] = [
        (
            "(".repeat(15) + "Vec<x>",
            15,
            &[
                (15, 18, Type),
                (18, 19, Punctuation),
                (19, 20, Type),
                (20, 21, Punctuation),
            ],
        ),
        (
            "(".repeat(16) + "Vec<x>",
            16,
            &[(16, 19, Type), (19, 20, Operator), (21, 22, Operator)],
        ),
        (
            "(".repeat(17) + &")".repeat(17) + "Vec<x>",
            34,
            &[
                (34, 37, Type),
                (37, 38, Punctuation),
                (38, 39, Type),
                (39, 40, Punctuation),
            ],
        ),
        (
            "(".repeat(17) + ">)Vec<x>",
            17,
            &[
                (17, 18, Operator),
                (18, 19, Punctuation),
                (19, 22, Type),
                (22, 23, Operator),
                (24, 25, Operator),
            ],
        ),
    ];
    for (text, brackets, tail) in cases {
        let mut expected: Vec<Token> = (0..brackets).map(|at| (at, at + 1, Punctuation)).collect();
        expected.extend_from_slice(tail);
        assert_eq!(tokens(&text), expected, "{text:?}");
    }
}

#[test]
fn a_generic_list_opens_after_a_type_a_type_keyword_a_fn_name_or_a_path_separator() {
    let p = Punctuation;
    check(&[
        (
            "Vec<Vec<T>>",
            &[
                (0, 3, Type),
                (3, 4, p),
                (4, 7, Type),
                (7, 8, p),
                (8, 9, Type),
                (9, 10, p),
                (10, 11, p),
            ],
        ),
        (
            "Box<dyn Fn(i32) -> i32>",
            &[
                (0, 3, Type),
                (3, 4, p),
                (4, 7, Keyword),
                (8, 10, Type),
                (10, 11, p),
                (11, 14, Type),
                (14, 15, p),
                (16, 18, Operator),
                (19, 22, Type),
                (22, 23, p),
            ],
        ),
        (
            "fn f<T>(x: T)",
            &[
                (0, 2, Keyword),
                (3, 4, Function),
                (4, 5, p),
                (5, 6, Type),
                (6, 7, p),
                (7, 8, p),
                (9, 10, p),
                (11, 12, Type),
                (12, 13, p),
            ],
        ),
        (
            "Vec::<x>::new()",
            &[
                (0, 3, Type),
                (3, 5, p),
                (5, 6, p),
                (6, 7, Type),
                (7, 8, p),
                (8, 10, p),
                (10, 13, Function),
                (13, 14, p),
                (14, 15, p),
            ],
        ),
        (
            "impl<T>",
            &[(0, 4, Keyword), (4, 5, p), (5, 6, Type), (6, 7, p)],
        ),
        (
            "Vec<a.b()>",
            &[
                (0, 3, Type),
                (3, 4, p),
                (4, 5, Type),
                (5, 6, Operator),
                (6, 7, Type),
                (7, 8, p),
                (8, 9, p),
                (9, 10, p),
            ],
        ),
        ("a < b > c", &[(2, 3, Operator), (6, 7, Operator)]),
    ]);
}

#[test]
fn a_brace_a_semicolon_and_a_closer_drop_the_open_generic_lists() {
    let p = Punctuation;
    check(&[
        (
            "if Foo < bar { x }",
            &[
                (0, 2, Keyword),
                (3, 6, Type),
                (7, 8, p),
                (9, 12, Type),
                (13, 14, p),
                (17, 18, p),
            ],
        ),
        (
            "Foo<x; y",
            &[(0, 3, Type), (3, 4, p), (4, 5, Type), (5, 6, p)],
        ),
        (
            "(Foo<Bar<x) y",
            &[
                (0, 1, p),
                (1, 4, Type),
                (4, 5, p),
                (5, 8, Type),
                (8, 9, p),
                (9, 10, Type),
                (10, 11, p),
            ],
        ),
    ]);
}

#[test]
fn an_attribute_colours_its_words_and_symbols_and_spans_lines() {
    let a = Attribute;
    check(&[
        (
            "#[derive(\nDebug)]\nfn",
            &[
                (0, 2, a),
                (2, 8, a),
                (8, 9, a),
                (10, 15, a),
                (15, 16, a),
                (16, 17, a),
                (18, 20, Keyword),
            ],
        ),
        (
            "#![allow(x)]",
            &[
                (0, 3, a),
                (3, 8, a),
                (8, 9, a),
                (9, 10, a),
                (10, 11, a),
                (11, 12, a),
            ],
        ),
        ("#[fn]", &[(0, 2, a), (2, 4, a), (4, 5, a)]),
    ]);
}

#[test]
fn an_attribute_keeps_the_class_of_its_literals_and_comments() {
    let a = Attribute;
    check(&[
        (
            "#[doc = \"s\"]",
            &[
                (0, 2, a),
                (2, 5, a),
                (6, 7, a),
                (8, 11, String),
                (11, 12, a),
            ],
        ),
        (
            "#[a(1, 'c', /* c */ [b])] fn",
            &[
                (0, 2, a),
                (2, 3, a),
                (3, 4, a),
                (4, 5, Constant),
                (5, 6, a),
                (7, 10, String),
                (10, 11, a),
                (12, 19, Comment),
                (20, 21, a),
                (21, 22, a),
                (22, 23, a),
                (23, 24, a),
                (24, 25, a),
                (26, 28, Keyword),
            ],
        ),
    ]);
}

#[test]
fn a_hash_opens_no_attribute_inside_one_or_with_a_full_stack() {
    let a = Attribute;
    check(&[
        (
            "#[a(#[b])] fn",
            &[
                (0, 2, a),
                (2, 3, a),
                (3, 4, a),
                (4, 5, a),
                (5, 6, a),
                (6, 7, a),
                (7, 8, a),
                (8, 9, a),
                (9, 10, a),
                (11, 13, Keyword),
            ],
        ),
        ("x # y #!z", &[(7, 8, Operator)]),
    ]);
    let text = "(".repeat(16) + "#[x] fn";
    let mut expected: Vec<Token> = (0..16).map(|at| (at, at + 1, Punctuation)).collect();
    expected.extend([
        (17, 18, Punctuation),
        (19, 20, Punctuation),
        (21, 23, Keyword),
    ]);
    assert_eq!(tokens(&text), expected);
}

#[test]
fn punctuation_and_operators_are_tokens() {
    let (p, o) = (Punctuation, Operator);
    check(&[
        ("a, b; c: d", &[(1, 2, p), (4, 5, p), (7, 8, p)]),
        ("a::b", &[(1, 3, p)]),
        ("(] )", &[(0, 1, p), (1, 2, p), (3, 4, p)]),
        ("x += !y && z", &[(2, 4, o), (5, 6, o), (8, 10, o)]),
        ("a -> b => c", &[(2, 4, o), (7, 9, o)]),
        (
            "a << b >> c <= d",
            &[
                (2, 3, o),
                (3, 4, o),
                (7, 8, o),
                (8, 9, o),
                (12, 13, o),
                (13, 14, o),
            ],
        ),
        (
            "a % b ^ c | d ~ e @ f",
            &[(2, 3, o), (6, 7, o), (10, 11, o), (14, 15, o), (18, 19, o)],
        ),
        ("a=//c", &[(1, 2, o), (2, 5, Comment)]),
        ("a-/*c*/b", &[(1, 2, o), (2, 7, Comment)]),
        ("$x", &[]),
    ]);
}

#[test]
fn a_dot_run_is_one_operator_and_a_lone_dot_keeps_the_look_behind() {
    let (p, o) = (Punctuation, Operator);
    check(&[
        ("a..b c...d", &[(1, 3, o), (6, 9, o)]),
        ("x?.y", &[(1, 2, o), (2, 3, o), (3, 4, Property)]),
        (
            "foo()?.bar()",
            &[
                (0, 3, Function),
                (3, 4, p),
                (4, 5, p),
                (5, 6, o),
                (6, 7, o),
                (7, 10, Function),
                (10, 11, p),
                (11, 12, p),
            ],
        ),
    ]);
}
