use crate::Language::Css;
use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String,
    Tag, Type,
};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Css, text, expected);
}

#[test]
fn a_block_comment_spans_lines() {
    check(
        "/* one\ntwo */ a {}",
        &[
            ("/* one", Comment),
            ("two */", Comment),
            ("a", Tag),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_selector_colours_tags_and_combinators_and_leaves_classes_ids_and_pseudos_plain() {
    check(
        ".card > h2:hover, #main * + &::after {",
        &[
            (".", Punctuation),
            (">", Operator),
            ("h2", Tag),
            (":", Punctuation),
            (",", Punctuation),
            ("#", Punctuation),
            ("*", Tag),
            ("+", Operator),
            ("&", Tag),
            ("::", Punctuation),
            ("{", Punctuation),
        ],
    );
}

#[test]
fn an_attribute_selector_colours_its_name_operator_and_value() {
    check(
        r#"a[href^="http"], [lang|=en] {"#,
        &[
            ("a", Tag),
            ("[", Punctuation),
            ("href", Attribute),
            ("^=", Operator),
            ("\"http\"", String),
            ("]", Punctuation),
            (",", Punctuation),
            ("[", Punctuation),
            ("lang", Attribute),
            ("|=", Operator),
            ("en", String),
            ("]", Punctuation),
            ("{", Punctuation),
        ],
    );
}

#[test]
fn a_declaration_colours_its_property_and_values() {
    check(
        "a { margin: 0 auto -1.5em; color: #3aa99f !important }",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("margin", Property),
            (":", Punctuation),
            ("0", Constant),
            ("auto", Constant),
            ("-1.5", Constant),
            ("em", Type),
            (";", Punctuation),
            ("color", Property),
            (":", Punctuation),
            ("#3aa99f", String),
            ("!important", Keyword),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_function_a_unit_and_a_custom_property_in_a_value() {
    check(
        "--gap: calc(100% - var(--x) * 2rem);",
        &[
            (":", Punctuation),
            ("calc", Function),
            ("(", Punctuation),
            ("100", Constant),
            ("%", Type),
            ("-", Operator),
            ("var", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("*", Operator),
            ("2", Constant),
            ("rem", Type),
            (")", Punctuation),
            (";", Punctuation),
        ],
    );
}

#[test]
fn an_unquoted_url_is_one_value_and_a_quoted_one_a_string() {
    check(
        r#"background: url(img/bg.png), url("a.png");"#,
        &[
            ("background", Property),
            (":", Punctuation),
            ("url", Function),
            ("(", Punctuation),
            ("img/bg.png", Constant),
            (")", Punctuation),
            (",", Punctuation),
            ("url", Function),
            ("(", Punctuation),
            ("\"a.png\"", String),
            (")", Punctuation),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_value_spans_lines_up_to_its_semicolon() {
    check(
        "font-family:\n  a, sans-serif;\nb {",
        &[
            ("font-family", Property),
            (":", Punctuation),
            ("a", Constant),
            (",", Punctuation),
            ("sans-serif", Constant),
            (";", Punctuation),
            ("b", Tag),
            ("{", Punctuation),
        ],
    );
}

#[test]
fn a_nested_rule_is_a_selector_and_a_declaration_a_property() {
    check(
        "a {\n  b:hover { x: 1 }\n  color: red\n}",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("b", Tag),
            (":", Punctuation),
            ("{", Punctuation),
            ("x", Property),
            (":", Punctuation),
            ("1", Constant),
            ("}", Punctuation),
            ("color", Property),
            (":", Punctuation),
            ("red", Constant),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_multi_line_selector_stays_a_selector() {
    check(
        "h1,\nh2 {",
        &[
            ("h1", Tag),
            (",", Punctuation),
            ("h2", Tag),
            ("{", Punctuation),
        ],
    );
}

#[test]
fn an_at_rule_colours_its_keyword_and_its_query() {
    check(
        "@media screen and (min-width: 600px) {",
        &[
            ("@media", Keyword),
            ("screen", Constant),
            ("and", Keyword),
            ("(", Punctuation),
            ("min-width", Property),
            (":", Punctuation),
            ("600", Constant),
            ("px", Type),
            (")", Punctuation),
            ("{", Punctuation),
        ],
    );
    check(
        r#"@import "theme.css";"#,
        &[
            ("@import", Keyword),
            ("\"theme.css\"", String),
            (";", Punctuation),
        ],
    );
}

#[test]
fn keyframes_take_a_name_and_their_selectors_are_keywords_and_percentages() {
    check(
        "@keyframes fade {\n  from { opacity: 0 }\n  50% { opacity: .5 }\n}",
        &[
            ("@keyframes", Keyword),
            ("fade", Constant),
            ("{", Punctuation),
            ("from", Keyword),
            ("{", Punctuation),
            ("opacity", Property),
            (":", Punctuation),
            ("0", Constant),
            ("}", Punctuation),
            ("50", Constant),
            ("%", Type),
            ("{", Punctuation),
            ("opacity", Property),
            (":", Punctuation),
            (".5", Constant),
            ("}", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_string_skips_escapes() {
    check(
        r#"content: "a\"b";"#,
        &[
            ("content", Property),
            (":", Punctuation),
            (r#""a\"b""#, String),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_nested_declaration_needs_no_space_after_its_colon_unless_a_comma_ends_its_line() {
    check(
        "a {\n  margin:0\n}\na {\n  b:hover,\n  i {",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("margin", Property),
            (":", Punctuation),
            ("0", Constant),
            ("}", Punctuation),
            ("a", Tag),
            ("{", Punctuation),
            ("b", Tag),
            (":", Punctuation),
            (",", Punctuation),
            ("i", Tag),
            ("{", Punctuation),
        ],
    );
}
