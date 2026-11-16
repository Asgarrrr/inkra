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

#[test]
fn a_number_takes_an_exponent_with_its_sign_and_a_unit_after_it() {
    check(
        "a { x: 1e3; y: 2.5E-2em; z: 4e+1px; w: 1em }",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("x", Property),
            (":", Punctuation),
            ("1e3", Constant),
            (";", Punctuation),
            ("y", Property),
            (":", Punctuation),
            ("2.5E-2", Constant),
            ("em", Type),
            (";", Punctuation),
            ("z", Property),
            (":", Punctuation),
            ("4e+1", Constant),
            ("px", Type),
            (";", Punctuation),
            ("w", Property),
            (":", Punctuation),
            ("1", Constant),
            ("em", Type),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_semicolon_or_a_brace_ends_a_declaration_and_a_quoted_brace_does_not_count() {
    check(
        "color:red;\na {\n  content: \"{\";\n}",
        &[
            ("color", Property),
            (":", Punctuation),
            ("red", Constant),
            (";", Punctuation),
            ("a", Tag),
            ("{", Punctuation),
            ("content", Property),
            (":", Punctuation),
            ("\"{\"", String),
            (";", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_selector_continues_to_its_end_even_where_a_name_and_colon_follow() {
    check(
        "a {\n  & b:hover\n}",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("&", Tag),
            ("b", Tag),
            (":", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_bang_before_another_word_is_an_operator() {
    check(
        "a { x: y !z }",
        &[
            ("a", Tag),
            ("{", Punctuation),
            ("x", Property),
            (":", Punctuation),
            ("y", Constant),
            ("!", Operator),
            ("z", Constant),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn an_at_rule_takes_comparisons_functions_and_feature_values() {
    check(
        "@media (400px <= width) and (orientation: landscape) {",
        &[
            ("@media", Keyword),
            ("(", Punctuation),
            ("400", Constant),
            ("px", Type),
            ("<", Operator),
            ("=", Operator),
            ("width", Constant),
            (")", Punctuation),
            ("and", Keyword),
            ("(", Punctuation),
            ("orientation", Property),
            (":", Punctuation),
            ("landscape", Constant),
            (")", Punctuation),
            ("{", Punctuation),
        ],
    );
    check(
        "@import url(\"a.css\") screen;",
        &[
            ("@import", Keyword),
            ("url", Function),
            ("(", Punctuation),
            ("\"a.css\"", String),
            (")", Punctuation),
            ("screen", Constant),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_pseudo_class_argument_nests_and_an_attribute_selector_takes_a_bare_equals() {
    check(
        "a:not(.b, .c) [type=text] {",
        &[
            ("a", Tag),
            (":", Punctuation),
            ("(", Punctuation),
            (".", Punctuation),
            (",", Punctuation),
            (".", Punctuation),
            (")", Punctuation),
            ("[", Punctuation),
            ("type", Attribute),
            ("=", Operator),
            ("text", String),
            ("]", Punctuation),
            ("{", Punctuation),
        ],
    );
}
