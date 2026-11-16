use crate::Language::Bash;
use crate::TokenClass::{self, Constant, Function, Keyword, Operator, Punctuation};
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
