use crate::Language::JavaScript;
use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String,
    Tag, Type,
};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(JavaScript, text, expected);
}

#[test]
fn an_escape_in_a_template_skips_one_byte_wherever_it_sits() {
    check(
        r"`abcdef\`g` + x",
        &[(r"`abcdef\`g`", String), ("+", Operator)],
    );
}

#[test]
fn comments_run_to_the_line_end_or_their_close_and_keep_the_look_behind() {
    check("a // b", &[("// b", Comment)]);
    check("a /* b */ c", &[("/* b */", Comment)]);
    check(
        "x /* one\ntwo */ / y",
        &[("/* one", Comment), ("two */", Comment), ("/", Operator)],
    );
}

#[test]
fn a_hashbang_line_is_a_comment() {
    check(
        "#!/usr/bin/env node\nx",
        &[("#!/usr/bin/env node", Comment)],
    );
}

#[test]
fn strings_skip_escapes_and_end_at_the_line_end_unclosed() {
    check(
        r#"'it\'s' + "a\"b""#,
        &[(r"'it\'s'", String), ("+", Operator), (r#""a\"b""#, String)],
    );
    check(
        "'abc\nx = 1",
        &[("'abc", String), ("=", Operator), ("1", Constant)],
    );
}

#[test]
fn a_template_spans_lines_and_its_substitutions_nest() {
    check(
        "s = `a ${f(`b ${c}`)} d\ne` / 2",
        &[
            ("=", Operator),
            ("`a ", String),
            ("${", Punctuation),
            ("f", Function),
            ("(", Punctuation),
            ("`b ", String),
            ("${", Punctuation),
            ("}", Punctuation),
            ("`", String),
            (")", Punctuation),
            ("}", Punctuation),
            (" d", String),
            ("e`", String),
            ("/", Operator),
            ("2", Constant),
        ],
    );
    check(
        r"`a\`b ${'}'}`",
        &[
            (r"`a\`b ", String),
            ("${", Punctuation),
            ("'}'", String),
            ("}", Punctuation),
            ("`", String),
        ],
    );
}

#[test]
fn a_slash_after_a_value_divides() {
    check("a / b / c", &[("/", Operator), ("/", Operator)]);
    check(
        "f(a) / a[0] / 2",
        &[
            ("f", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("/", Operator),
            ("[", Punctuation),
            ("0", Constant),
            ("]", Punctuation),
            ("/", Operator),
            ("2", Constant),
        ],
    );
    check("x++ / y", &[("++", Operator), ("/", Operator)]);
}

#[test]
fn a_slash_at_an_expression_start_opens_a_regex_with_its_class_and_flags() {
    check(
        r"x = /re[/]\//gi.test(s)",
        &[
            ("=", Operator),
            (r"/re[/]\//", String),
            ("gi", Keyword),
            (".", Punctuation),
            ("test", Function),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
    check("return /x/", &[("return", Keyword), ("/x/", String)]);
    check(
        "if (ok) /x/.test(s)",
        &[
            ("if", Keyword),
            ("(", Punctuation),
            (")", Punctuation),
            ("/x/", String),
            (".", Punctuation),
            ("test", Function),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
    check("++/x/", &[("++", Operator), ("/x/", String)]);
    check("a = /x\nb", &[("=", Operator), ("/x", String)]);
}

#[test]
fn a_less_than_after_a_value_compares_and_at_an_expression_start_opens_a_tag() {
    check("a < b", &[("<", Operator)]);
    check(
        "x <= y << 2",
        &[("<=", Operator), ("<<", Operator), ("2", Constant)],
    );
    check(
        "return <div>",
        &[
            ("return", Keyword),
            ("<", Punctuation),
            ("div", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_jsx_element_colours_its_tags_attributes_and_children() {
    check(
        r#"<div className="x" onClick={go}>hi {name} &amp; <br/></div>"#,
        &[
            ("<", Punctuation),
            ("div", Tag),
            ("className", Attribute),
            ("=", Punctuation),
            ("\"x\"", String),
            ("onClick", Attribute),
            ("=", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            (">", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            ("&amp;", String),
            ("<", Punctuation),
            ("br", Tag),
            ("/>", Punctuation),
            ("</", Punctuation),
            ("div", Tag),
            (">", Punctuation),
        ],
    );
}

#[test]
fn a_jsx_tag_and_its_children_span_lines() {
    check(
        "<Foo.Bar\n  disabled\n  data-id='a'\n>\n  it's text\n</Foo.Bar>;\nx / 2",
        &[
            ("<", Punctuation),
            ("Foo", Tag),
            (".", Punctuation),
            ("Bar", Tag),
            ("disabled", Attribute),
            ("data-id", Attribute),
            ("=", Punctuation),
            ("'a'", String),
            (">", Punctuation),
            ("</", Punctuation),
            ("Foo", Tag),
            (".", Punctuation),
            ("Bar", Tag),
            (">", Punctuation),
            (";", Punctuation),
            ("/", Operator),
            ("2", Constant),
        ],
    );
}

#[test]
fn a_fragment_and_an_element_inside_an_expression_nest() {
    check(
        "<>\n  <ul>{xs.map(x => <li key={x}>{x}</li>)}</ul>\n</>",
        &[
            ("<", Punctuation),
            (">", Punctuation),
            ("<", Punctuation),
            ("ul", Tag),
            (">", Punctuation),
            ("{", Punctuation),
            (".", Punctuation),
            ("map", Function),
            ("(", Punctuation),
            ("=>", Operator),
            ("<", Punctuation),
            ("li", Tag),
            ("key", Attribute),
            ("=", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            (">", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            ("</", Punctuation),
            ("li", Tag),
            (">", Punctuation),
            (")", Punctuation),
            ("}", Punctuation),
            ("</", Punctuation),
            ("ul", Tag),
            (">", Punctuation),
            ("</", Punctuation),
            (">", Punctuation),
        ],
    );
}

#[test]
fn numbers_take_separators_bigints_radixes_exponents_and_a_leading_dot() {
    check(
        "1_000n + 0xFF + .5 + 1e-3",
        &[
            ("1_000n", Constant),
            ("+", Operator),
            ("0xFF", Constant),
            ("+", Operator),
            (".5", Constant),
            ("+", Operator),
            ("1e-3", Constant),
        ],
    );
}

#[test]
fn reserved_words_are_keywords_and_a_word_containing_one_is_not() {
    check(
        "const x = await import('m')",
        &[
            ("const", Keyword),
            ("=", Operator),
            ("await", Keyword),
            ("import", Keyword),
            ("(", Punctuation),
            ("'m'", String),
            (")", Punctuation),
        ],
    );
    check("define iffy", &[]);
}

#[test]
fn a_soft_keyword_is_a_keyword_only_before_an_operand() {
    check(
        "import { a as b } from 'm'",
        &[
            ("import", Keyword),
            ("{", Punctuation),
            ("as", Keyword),
            ("}", Punctuation),
            ("from", Keyword),
            ("'m'", String),
        ],
    );
    check(
        "let of = get(set.of)",
        &[
            ("let", Keyword),
            ("=", Operator),
            ("get", Function),
            ("(", Punctuation),
            (".", Punctuation),
            ("of", Property),
            (")", Punctuation),
        ],
    );
}

#[test]
fn a_word_after_a_dot_is_a_property_or_a_method_even_a_keyword() {
    check(
        "p.catch(e).finally.x",
        &[
            (".", Punctuation),
            ("catch", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (".", Punctuation),
            ("finally", Property),
            (".", Punctuation),
            ("x", Property),
        ],
    );
}

#[test]
fn a_function_name_follows_function_or_comes_before_a_call_or_a_template() {
    check(
        "async function load(url) {}",
        &[
            ("async", Keyword),
            ("function", Keyword),
            ("load", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
    check("html`<b>`", &[("html", Function), ("`<b>`", String)]);
}

#[test]
fn an_object_key_is_a_property_and_a_ternary_branch_is_not() {
    check(
        "({ a: 1, 'b': 2, c })",
        &[
            ("(", Punctuation),
            ("{", Punctuation),
            ("a", Property),
            (":", Punctuation),
            ("1", Constant),
            (",", Punctuation),
            ("'b'", String),
            (":", Punctuation),
            ("2", Constant),
            (",", Punctuation),
            ("}", Punctuation),
            (")", Punctuation),
        ],
    );
    check(
        "{ x ? a : b }",
        &[
            ("{", Punctuation),
            ("?", Operator),
            (":", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn optional_chaining_is_a_dot_unless_a_digit_follows() {
    check(
        "a?.b?.(c)",
        &[
            ("?.", Punctuation),
            ("b", Property),
            ("?.", Punctuation),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
    check(
        "a?.5:1",
        &[
            ("?", Operator),
            (".5", Constant),
            (":", Punctuation),
            ("1", Constant),
        ],
    );
}

#[test]
fn literals_and_all_caps_names_are_constants() {
    check(
        "x = null ?? undefined ?? MAX_SIZE ?? A ?? true",
        &[
            ("=", Operator),
            ("null", Constant),
            ("??", Operator),
            ("undefined", Constant),
            ("??", Operator),
            ("MAX_SIZE", Constant),
            ("??", Operator),
            ("A", Constant),
            ("??", Operator),
            ("true", Constant),
        ],
    );
}

#[test]
fn a_class_its_base_a_constructor_and_a_builtin_object_are_types() {
    check(
        "class Foo extends Bar {}",
        &[
            ("class", Keyword),
            ("Foo", Type),
            ("extends", Keyword),
            ("Bar", Type),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
    check(
        "new Map(); Promise.all(xs); Foo.bar",
        &[
            ("new", Keyword),
            ("Map", Type),
            ("(", Punctuation),
            (")", Punctuation),
            (";", Punctuation),
            ("Promise", Type),
            (".", Punctuation),
            ("all", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (";", Punctuation),
            (".", Punctuation),
            ("bar", Property),
        ],
    );
    check(
        "constructor(x) {}",
        &[
            ("constructor", Type),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
    check(
        "extends React.Component",
        &[
            ("extends", Keyword),
            (".", Punctuation),
            ("Component", Property),
        ],
    );
}

#[test]
fn a_decorator_is_an_attribute_and_a_private_name_a_property() {
    check(
        "@observable.deep #count = 0; this.#inc()",
        &[
            ("@observable.deep", Attribute),
            ("#count", Property),
            ("=", Operator),
            ("0", Constant),
            (";", Punctuation),
            (".", Punctuation),
            ("#inc", Function),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn this_and_super_stay_plain() {
    check(
        "super(x); this.y",
        &[
            ("(", Punctuation),
            (")", Punctuation),
            (";", Punctuation),
            (".", Punctuation),
            ("y", Property),
        ],
    );
}

#[test]
fn operators_take_the_longest_match() {
    check(
        "(...xs) => a === b !== c >>>= d",
        &[
            ("(", Punctuation),
            ("...", Operator),
            (")", Punctuation),
            ("=>", Operator),
            ("===", Operator),
            ("!==", Operator),
            (">>>=", Operator),
        ],
    );
}

#[test]
fn a_closing_brace_ends_no_value() {
    check(
        "if (x) {} /re/.test(y)\n({}) / 2",
        &[
            ("if", Keyword),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            ("/re/", String),
            (".", Punctuation),
            ("test", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("(", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
            (")", Punctuation),
            ("/", Operator),
            ("2", Constant),
        ],
    );
}

#[test]
fn a_tag_past_the_nesting_room_is_operators_and_the_next_line_still_colours() {
    let room = crate::state::LineState::default().stack.len();
    let text = format!("{}<div>\nconst a = 1;", "(".repeat(room));
    let expected: Vec<_> = std::iter::repeat_n(("(", Punctuation), room)
        .chain([
            ("<", Operator),
            (">", Operator),
            ("const", Keyword),
            ("=", Operator),
            ("1", Constant),
            (";", Punctuation),
        ])
        .collect();
    check(&text, &expected);
}

#[test]
fn a_function_name_is_a_function_with_its_parameters_on_the_next_line() {
    check(
        "function load\n(x) {}",
        &[
            ("function", Keyword),
            ("load", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn a_private_name_at_the_line_start_is_no_hashbang() {
    check(
        "#count = 0",
        &[("#count", Property), ("=", Operator), ("0", Constant)],
    );
}

#[test]
fn a_key_named_constructor_is_a_property_and_a_key_value_stays_plain() {
    check(
        "({ constructor: 1, key: value })",
        &[
            ("(", Punctuation),
            ("{", Punctuation),
            ("constructor", Property),
            (":", Punctuation),
            ("1", Constant),
            (",", Punctuation),
            ("key", Property),
            (":", Punctuation),
            ("}", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn optional_chaining_at_the_line_end_takes_the_property_on_the_next_line() {
    check("a?.\nb", &[("?.", Punctuation), ("b", Property)]);
}

#[test]
fn a_dotted_tag_name_and_a_spaced_self_closing_slash() {
    check(
        "x = <a.b.c / d>t</a.b.c>;\ny = <br / >;",
        &[
            ("=", Operator),
            ("<", Punctuation),
            ("a", Tag),
            (".", Punctuation),
            ("b", Tag),
            (".", Punctuation),
            ("c", Tag),
            ("d", Attribute),
            (">", Punctuation),
            ("</", Punctuation),
            ("a", Tag),
            (".", Punctuation),
            ("b", Tag),
            (".", Punctuation),
            ("c", Tag),
            (">", Punctuation),
            (";", Punctuation),
            ("=", Operator),
            ("<", Punctuation),
            ("br", Tag),
            ("/ >", Punctuation),
            (";", Punctuation),
        ],
    );
}

#[test]
fn a_fragment_child_opens_and_a_child_past_the_nesting_room_stays_text() {
    check(
        "z = <a><>t</></a>;",
        &[
            ("=", Operator),
            ("<", Punctuation),
            ("a", Tag),
            (">", Punctuation),
            ("<", Punctuation),
            (">", Punctuation),
            ("</", Punctuation),
            (">", Punctuation),
            ("</", Punctuation),
            ("a", Tag),
            (">", Punctuation),
            (";", Punctuation),
        ],
    );
    let room = crate::state::LineState::default().stack.len();
    let opened = [("<", Punctuation), ("b", Tag), (">", Punctuation)];
    let expected: Vec<_> = opened.iter().copied().cycle().take(3 * room).collect();
    check(&format!("{}<i>", "<b>".repeat(room)), &expected);
}
