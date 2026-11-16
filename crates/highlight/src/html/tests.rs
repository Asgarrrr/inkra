use crate::Language::Html;
use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String,
    Tag,
};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Html, text, expected);
}

#[test]
fn a_tag_colours_its_name_attributes_and_values() {
    check(
        r#"<a href="x" data-id='7' hidden rel=me>go</a>"#,
        &[
            ("<", Punctuation),
            ("a", Tag),
            ("href", Attribute),
            ("=", Punctuation),
            ("\"x\"", String),
            ("data-id", Attribute),
            ("=", Punctuation),
            ("'7'", String),
            ("hidden", Attribute),
            ("rel", Attribute),
            ("=", Punctuation),
            ("me", String),
            (">", Punctuation),
            ("</", Punctuation),
            ("a", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_tag_and_a_quoted_value_span_lines() {
    check(
        "<body class=\"a\n b\"\n  id=x>t",
        &[
            ("<", Punctuation),
            ("body", Tag),
            ("class", Attribute),
            ("=", Punctuation),
            ("\"a", String),
            (" b\"", String),
            ("id", Attribute),
            ("=", Punctuation),
            ("x", String),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_self_closing_tag_ends_with_its_slash() {
    check(
        "<br/> <img src=a />",
        &[
            ("<", Punctuation),
            ("br", Tag),
            ("/>", Punctuation),
            ("<", Punctuation),
            ("img", Tag),
            ("src", Attribute),
            ("=", Punctuation),
            ("a", String),
            ("/>", Punctuation),
        ],
    );
}

#[test]
fn an_entity_is_a_string_and_a_bare_ampersand_or_less_than_is_text() {
    check(
        "Fish &amp; chips &#169; & a < b",
        &[("&amp;", String), ("&#169;", String)],
    );
}

#[test]
fn a_comment_spans_lines() {
    check(
        "<!-- one\ntwo --> <p>",
        &[
            ("<!-- one", Comment),
            ("two -->", Comment),
            ("<", Punctuation),
            ("p", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn the_doctype_is_a_tag_between_brackets() {
    check(
        "<!DOCTYPE html>",
        &[
            ("<!", Punctuation),
            ("DOCTYPE html", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn script_content_lexes_as_javascript_across_lines_up_to_its_closing_tag() {
    check(
        "<script type=\"module\">\n  /* a */ if (x < 1) /re/.test(s)\n</SCRIPT><p>",
        &[
            ("<", Punctuation),
            ("script", Tag),
            ("type", Attribute),
            ("=", Punctuation),
            ("\"module\"", String),
            (">", Punctuation),
            ("/* a */", Comment),
            ("if", Keyword),
            ("(", Punctuation),
            ("<", Operator),
            ("1", Constant),
            (")", Punctuation),
            ("/re/", String),
            (".", Punctuation),
            ("test", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("</", Punctuation),
            ("SCRIPT", Tag),
            (">", Punctuation),
            ("<", Punctuation),
            ("p", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_script_comment_left_open_ends_with_the_script() {
    check(
        "<script>/* a\n</script> <b>",
        &[
            ("<", Punctuation),
            ("script", Tag),
            (">", Punctuation),
            ("/* a", Comment),
            ("</", Punctuation),
            ("script", Tag),
            (">", Punctuation),
            ("<", Punctuation),
            ("b", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn style_content_lexes_as_css_on_its_tag_line_too() {
    check(
        "<style>a { color: red }\n.b { x: 1 }</style>",
        &[
            ("<", Punctuation),
            ("style", Tag),
            (">", Punctuation),
            ("a", Tag),
            ("{", Punctuation),
            ("color", Property),
            (":", Punctuation),
            ("red", Constant),
            ("}", Punctuation),
            (".", Punctuation),
            ("{", Punctuation),
            ("x", Property),
            (":", Punctuation),
            ("1", Constant),
            ("}", Punctuation),
            ("</", Punctuation),
            ("style", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_closing_script_tag_opens_no_javascript() {
    check(
        "</script> a / b",
        &[("</", Punctuation), ("script", Tag), (">", Punctuation)],
    );
}

#[test]
fn an_unquoted_value_runs_to_a_space_or_the_tag_end_through_slashes() {
    check(
        "<a href=/about/team rel=x>",
        &[
            ("<", Punctuation),
            ("a", Tag),
            ("href", Attribute),
            ("=", Punctuation),
            ("/about/team", String),
            ("rel", Attribute),
            ("=", Punctuation),
            ("x", String),
            (">", Punctuation),
        ],
    );
}
