use inkra_text::ByteOffset;

use crate::Language::Python;
use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String,
    Type,
};
use crate::fixtures::{expect, r};
use crate::highlight;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Python, text, expected);
}

#[test]
fn a_comment_runs_to_the_line_end_but_not_from_inside_a_string() {
    check(
        "x = '#' # a note",
        &[("=", Operator), ("'#'", String), ("# a note", Comment)],
    );
}

#[test]
fn keywords_are_tokens_and_a_keyword_inside_a_word_is_none() {
    check(
        "async def f(): return not x in y is None or lambda: await z",
        &[
            ("async", Keyword),
            ("def", Keyword),
            ("f", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
            ("return", Keyword),
            ("not", Keyword),
            ("in", Keyword),
            ("is", Keyword),
            ("None", Constant),
            ("or", Keyword),
            ("lambda", Keyword),
            (":", Punctuation),
            ("await", Keyword),
        ],
    );
    check("define iffy is_x", &[]);
}

#[test]
fn print_and_exec_are_builtins_not_keywords() {
    check("print exec", &[]);
}

#[test]
fn a_soft_keyword_is_a_keyword_only_where_it_starts_a_statement() {
    check("match command:", &[("match", Keyword), (":", Punctuation)]);
    check(
        "    case [a]:",
        &[
            ("case", Keyword),
            ("[", Punctuation),
            ("]", Punctuation),
            (":", Punctuation),
        ],
    );
    check(
        "type Alias = int",
        &[
            ("type", Keyword),
            ("Alias", Type),
            ("=", Operator),
            ("int", Type),
        ],
    );
    check("match = 1", &[("=", Operator), ("1", Constant)]);
    check("match.group", &[(".", Punctuation), ("group", Property)]);
    check(
        "match(x)",
        &[("match", Function), ("(", Punctuation), (")", Punctuation)],
    );
    check("match", &[]);
    check(
        "if type in kinds:",
        &[("if", Keyword), ("in", Keyword), (":", Punctuation)],
    );
    check(
        "x = type(y)",
        &[
            ("=", Operator),
            ("type", Function),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn a_string_takes_either_quote_and_its_escapes() {
    check(
        r#"'a' "b" 'it\'s' "say \"hi\"""#,
        &[
            ("'a'", String),
            (r#""b""#, String),
            (r"'it\'s'", String),
            (r#""say \"hi\"""#, String),
        ],
    );
}

#[test]
fn a_string_takes_its_prefix_in_any_case() {
    check(
        r#"r'\d' b"x" f'{a}' Rb'y' BR"z" u'w' t'v' fr"q""#,
        &[
            (r"r'\d'", String),
            (r#"b"x""#, String),
            ("f'{a}'", String),
            ("Rb'y'", String),
            (r#"BR"z""#, String),
            ("u'w'", String),
            ("t'v'", String),
            (r#"fr"q""#, String),
        ],
    );
}

#[test]
fn a_word_that_is_no_prefix_stays_apart_from_its_string() {
    check(
        "rb = x'a' + bu'b'",
        &[
            ("=", Operator),
            ("'a'", String),
            ("+", Operator),
            ("'b'", String),
        ],
    );
}

#[test]
fn a_triple_quoted_string_spans_lines_one_token_per_line() {
    check(
        "s = \"\"\"one\ntwo ' \" ''' \n\nthree\"\"\" + x",
        &[
            ("=", Operator),
            ("\"\"\"one", String),
            ("two ' \" ''' ", String),
            ("three\"\"\"", String),
            ("+", Operator),
        ],
    );
    check("'''a \\''' b'''\nc", &[("'''a \\''' b'''", String)]);
}

#[test]
fn highlight_carries_a_triple_quoted_string_to_the_next_line_at_its_offset() {
    let mut out = Vec::new();
    let at = ByteOffset::new;
    highlight(
        Python,
        [(at(0), "x = '''a"), (at(20), "b''' # c")].into_iter(),
        &mut out,
    );
    assert_eq!(
        out,
        [
            (r(2, 3), Operator),
            (r(4, 8), String),
            (r(20, 24), String),
            (r(25, 28), Comment),
        ]
    );
}

#[test]
fn an_unclosed_single_quoted_string_ends_at_the_line_end() {
    check(
        "x = 'abc\ny = 1",
        &[
            ("=", Operator),
            ("'abc", String),
            ("=", Operator),
            ("1", Constant),
        ],
    );
}

#[test]
fn numbers_are_constants() {
    check(
        "1_000 0x1F 1.5e-3 3j .5 1. 0o7",
        &[
            ("1_000", Constant),
            ("0x1F", Constant),
            ("1.5e-3", Constant),
            ("3j", Constant),
            (".5", Constant),
            ("1.", Constant),
            ("0o7", Constant),
        ],
    );
}

#[test]
fn literals_all_caps_names_and_the_ellipsis_are_constants() {
    check(
        "True False None MAX_SIZE _PRIVATE_KEY ... X",
        &[
            ("True", Constant),
            ("False", Constant),
            ("None", Constant),
            ("MAX_SIZE", Constant),
            ("_PRIVATE_KEY", Constant),
            ("...", Constant),
            ("X", Constant),
        ],
    );
}

#[test]
fn a_name_after_def_is_a_function_and_after_class_a_type() {
    check(
        "def run(self): pass",
        &[
            ("def", Keyword),
            ("run", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
            ("pass", Keyword),
        ],
    );
    check(
        "class lower(Base):",
        &[
            ("class", Keyword),
            ("lower", Type),
            ("(", Punctuation),
            ("Base", Type),
            (")", Punctuation),
            (":", Punctuation),
        ],
    );
    check(
        "def Handler(): ...",
        &[
            ("def", Keyword),
            ("Handler", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
            ("...", Constant),
        ],
    );
    check(
        "def first[T](items):",
        &[
            ("def", Keyword),
            ("first", Function),
            ("[", Punctuation),
            ("T", Constant),
            ("]", Punctuation),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
        ],
    );
    check(
        "def f(x): y",
        &[
            ("def", Keyword),
            ("f", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (":", Punctuation),
        ],
    );
}

#[test]
fn a_name_before_a_paren_on_its_line_is_a_function() {
    check(
        "print(len (xs)) naïve()",
        &[
            ("print", Function),
            ("(", Punctuation),
            ("len", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (")", Punctuation),
            ("naïve", Function),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
    check("f\n(x)", &[("(", Punctuation), (")", Punctuation)]);
}

#[test]
fn capitalised_names_and_builtin_types_are_types() {
    check(
        "x: int = Foo() or str(y) or _Private",
        &[
            (":", Punctuation),
            ("int", Type),
            ("=", Operator),
            ("Foo", Type),
            ("(", Punctuation),
            (")", Punctuation),
            ("or", Keyword),
            ("str", Type),
            ("(", Punctuation),
            (")", Punctuation),
            ("or", Keyword),
            ("_Private", Type),
        ],
    );
    check("__init__ _", &[]);
}

#[test]
fn after_a_dot_a_call_is_a_function_and_else_a_property() {
    check(
        "obj.attr obj.method () self.MAX",
        &[
            (".", Punctuation),
            ("attr", Property),
            (".", Punctuation),
            ("method", Function),
            ("(", Punctuation),
            (")", Punctuation),
            (".", Punctuation),
            ("MAX", Property),
        ],
    );
}

#[test]
fn a_decorator_at_a_line_start_is_an_attribute_with_its_dotted_name() {
    check(
        "@property\n@v1.route('/x')\n    @wraps(f)",
        &[
            ("@property", Attribute),
            ("@v1.route", Attribute),
            ("(", Punctuation),
            ("'/x'", String),
            (")", Punctuation),
            ("@wraps", Attribute),
            ("(", Punctuation),
            (")", Punctuation),
        ],
    );
}

#[test]
fn an_at_sign_inside_an_expression_is_an_operator() {
    check(
        "x = a @ b @c",
        &[("=", Operator), ("@", Operator), ("@", Operator)],
    );
    check("@ x", &[("@", Operator)]);
}

#[test]
fn punctuation_and_operators_are_tokens() {
    check(
        "a[0], {k: v}; (n := 1) -> x ** 2 != y",
        &[
            ("[", Punctuation),
            ("0", Constant),
            ("]", Punctuation),
            (",", Punctuation),
            ("{", Punctuation),
            (":", Punctuation),
            ("}", Punctuation),
            (";", Punctuation),
            ("(", Punctuation),
            (":=", Operator),
            ("1", Constant),
            (")", Punctuation),
            ("->", Operator),
            ("**", Operator),
            ("2", Constant),
            ("!=", Operator),
        ],
    );
    check("x \\ $ ?", &[]);
}
