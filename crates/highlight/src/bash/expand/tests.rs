use crate::Language::Bash;
use crate::TokenClass::{self, Function, Punctuation, String};
use crate::fixtures::{expect, tokens};

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Bash, text, expected);
}

#[test]
fn a_single_quoted_string_has_no_escapes() {
    check(
        r"echo 'a\' b",
        &[("echo", Function), (r"'a\'", String), ("b", String)],
    );
}

#[test]
fn a_single_quoted_string_spans_lines_and_the_command_goes_on() {
    check(
        "echo 'one\ntwo' x",
        &[
            ("echo", Function),
            ("'one", String),
            ("two'", String),
            ("x", String),
        ],
    );
}

#[test]
fn an_ansi_c_string_takes_escapes() {
    check(
        r"echo $'a\'b' c",
        &[("echo", Function), (r"$'a\'b'", String), ("c", String)],
    );
    check(
        r"echo $'it\'s' x",
        &[("echo", Function), (r"$'it\'s'", String), ("x", String)],
    );
}

#[test]
fn a_locale_string_is_one_string_with_its_dollar() {
    check(
        r#"echo $"a" "b c""#,
        &[
            ("echo", Function),
            (r#"$"a""#, String),
            (r#""b c""#, String),
        ],
    );
}

#[test]
fn a_double_quoted_string_takes_escapes_and_lights_its_parameters() {
    check(
        r#"echo "a \"$USER\" ${HOME} $1 $@ $(date) $""#,
        &[
            ("echo", Function),
            (r#""a \""#, String),
            ("$", Punctuation),
            (r#"\" "#, String),
            ("${", Punctuation),
            ("}", Punctuation),
            (" ", String),
            ("$", Punctuation),
            (" ", String),
            ("$", Punctuation),
            (r#" $(date) $""#, String),
        ],
    );
}

#[test]
fn a_double_quoted_string_spans_lines() {
    check(
        "echo \"one\ntwo $x\" y",
        &[
            ("echo", Function),
            ("\"one", String),
            ("two ", String),
            ("$", Punctuation),
            ("\"", String),
            ("y", String),
        ],
    );
}

#[test]
fn parameters_outside_strings_are_punctuation_and_a_plain_name() {
    check(
        "echo $HOME ${x:-d} ${#a[@]} $?",
        &[
            ("echo", Function),
            ("$", Punctuation),
            ("${", Punctuation),
            ("}", Punctuation),
            ("${", Punctuation),
            ("}", Punctuation),
            ("$", Punctuation),
        ],
    );
    check(
        "echo $$ x",
        &[("echo", Function), ("$", Punctuation), ("x", String)],
    );
    check(
        "echo $ a$b",
        &[
            ("echo", Function),
            ("$", String),
            ("a", String),
            ("$", Punctuation),
        ],
    );
}

#[test]
fn a_braced_parameter_closes_on_its_own_brace() {
    assert_eq!(
        tokens(Bash, "echo ${a:-${b}} x"),
        [
            (0, 4, Function),
            (5, 7, Punctuation),
            (14, 15, Punctuation),
            (16, 17, String),
        ]
    );
    check("echo ${open", &[("echo", Function), ("${", Punctuation)]);
}
