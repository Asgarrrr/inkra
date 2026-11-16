use crate::Language::Bash;
use crate::TokenClass::{self, Constant, Function, Keyword, Operator, Punctuation, String};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Bash, text, expected);
}

#[test]
fn arithmetic_keeps_names_plain_and_takes_no_heredoc() {
    check(
        "echo $(( (a + 1) << 2 ))\nnext",
        &[
            ("echo", Function),
            ("$((", Punctuation),
            ("(", Punctuation),
            ("+", Operator),
            ("1", Constant),
            (")", Punctuation),
            ("<<", Operator),
            ("2", Constant),
            ("))", Punctuation),
            ("next", Function),
        ],
    );
    check(
        "for ((i = 0; i < $n; i++)); do",
        &[
            ("for", Keyword),
            ("((", Punctuation),
            ("=", Operator),
            ("0", Constant),
            (";", Punctuation),
            ("<", Operator),
            ("$", Punctuation),
            (";", Punctuation),
            ("++", Operator),
            ("))", Punctuation),
            (";", Punctuation),
            ("do", Keyword),
        ],
    );
}

#[test]
fn arithmetic_lexes_a_quoted_string_whole() {
    check(
        "echo $(( \"a b\" + 'c d' ))",
        &[
            ("echo", Function),
            ("$((", Punctuation),
            ("\"a b\"", String),
            ("+", Operator),
            ("'c d'", String),
            ("))", Punctuation),
        ],
    );
}

#[test]
fn arithmetic_keeps_a_name_with_digits_plain_and_a_number_whole() {
    check(
        "(( x1 + 42 ))",
        &[
            ("((", Punctuation),
            ("+", Operator),
            ("42", Constant),
            ("))", Punctuation),
        ],
    );
}

#[test]
fn only_a_double_paren_closes_arithmetic() {
    check(
        "(( a ) + b ))",
        &[
            ("((", Punctuation),
            (")", Punctuation),
            ("+", Operator),
            ("))", Punctuation),
        ],
    );
}

#[test]
fn a_dollar_that_starts_no_parameter_stays_plain_in_arithmetic() {
    check(
        "echo $(( 1 + $ ))",
        &[
            ("echo", Function),
            ("$((", Punctuation),
            ("1", Constant),
            ("+", Operator),
            ("))", Punctuation),
        ],
    );
}
