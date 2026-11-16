use crate::Language::Json;
use crate::TokenClass::{self, Comment, Constant, Property, Punctuation, String};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Json, text, expected);
}

#[test]
fn a_key_is_a_property_and_a_value_a_string() {
    check(
        r#"{"name": "inkra", "tags" :["a"]}"#,
        &[
            ("{", Punctuation),
            (r#""name""#, Property),
            (":", Punctuation),
            (r#""inkra""#, String),
            (",", Punctuation),
            (r#""tags""#, Property),
            (":", Punctuation),
            ("[", Punctuation),
            (r#""a""#, String),
            ("]", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_string_takes_its_escapes_and_ends_at_the_line_end_when_unclosed() {
    check(
        r#""a\"b": "c\\" "open"#,
        &[
            (r#""a\"b""#, Property),
            (":", Punctuation),
            (r#""c\\""#, String),
            (r#""open"#, String),
        ],
    );
    check("\"open\n1", &[("\"open", String), ("1", Constant)]);
}

#[test]
fn a_key_needs_its_colon_on_the_same_line() {
    check(
        "\"k\"\n: 1",
        &[("\"k\"", String), (":", Punctuation), ("1", Constant)],
    );
}

#[test]
fn numbers_with_their_sign_and_literals_are_constants() {
    check(
        "[-1, 2.5e+10, 0, true, false, null, nul, -x]",
        &[
            ("[", Punctuation),
            ("-1", Constant),
            (",", Punctuation),
            ("2.5e+10", Constant),
            (",", Punctuation),
            ("0", Constant),
            (",", Punctuation),
            ("true", Constant),
            (",", Punctuation),
            ("false", Constant),
            (",", Punctuation),
            ("null", Constant),
            (",", Punctuation),
            (",", Punctuation),
            ("]", Punctuation),
        ],
    );
}

#[test]
fn line_and_block_comments_are_comments_and_a_block_one_spans_lines() {
    check(
        "{ // a note\n  /* one\n two */ \"k\": 1 /* x */\n}",
        &[
            ("{", Punctuation),
            ("// a note", Comment),
            ("/* one", Comment),
            (" two */", Comment),
            ("\"k\"", Property),
            (":", Punctuation),
            ("1", Constant),
            ("/* x */", Comment),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_block_comment_does_not_nest() {
    check(
        "/* a /* b */ 1 */",
        &[("/* a /* b */", Comment), ("1", Constant)],
    );
}

#[test]
fn a_comment_opener_inside_a_string_is_text() {
    check(
        r#""http://x/*": 1"#,
        &[
            (r#""http://x/*""#, Property),
            (":", Punctuation),
            ("1", Constant),
        ],
    );
}
