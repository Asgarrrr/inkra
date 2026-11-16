use crate::Language::Bash;
use crate::TokenClass::{
    self, Comment, Constant, Function, Keyword, Operator, Punctuation, String,
};
use crate::fixtures::expect;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Bash, text, expected);
}

#[test]
fn a_command_name_is_a_function_and_its_arguments_strings_flags_and_numbers_constants() {
    check(
        "echo hello -n --to=x 42 ./x 4a",
        &[
            ("echo", Function),
            ("hello", String),
            ("-n", Constant),
            ("--to=x", Constant),
            ("42", Constant),
            ("./x", String),
            ("4a", String),
        ],
    );
}

#[test]
fn a_separator_starts_a_command_again() {
    check(
        "a | b && c; d & e || f |& g",
        &[
            ("a", Function),
            ("|", Operator),
            ("b", Function),
            ("&&", Operator),
            ("c", Function),
            (";", Punctuation),
            ("d", Function),
            ("&", Operator),
            ("e", Function),
            ("||", Operator),
            ("f", Function),
            ("|&", Operator),
            ("g", Function),
        ],
    );
}

#[test]
fn a_line_end_starts_a_command_unless_a_backslash_continues_it() {
    check(
        "ls -l\ncd /tmp",
        &[
            ("ls", Function),
            ("-l", Constant),
            ("cd", Function),
            ("/tmp", String),
        ],
    );
    check(
        "ls \\\n  -l\nx \\\\\ny",
        &[
            ("ls", Function),
            ("-l", Constant),
            ("x", Function),
            ("\\\\", String),
            ("y", Function),
        ],
    );
    check(
        "ls # a \\\npwd",
        &[("ls", Function), ("# a \\", Comment), ("pwd", Function)],
    );
}

#[test]
fn a_comment_starts_only_a_word() {
    check(
        "echo a#b # note\n# whole line",
        &[
            ("echo", Function),
            ("a#b", String),
            ("# note", Comment),
            ("# whole line", Comment),
        ],
    );
}

#[test]
fn a_reserved_word_is_a_keyword_only_where_a_command_starts() {
    check(
        "if true; then :; elif x; then exit 1; else y; fi",
        &[
            ("if", Keyword),
            ("true", Function),
            (";", Punctuation),
            ("then", Keyword),
            (":", Function),
            (";", Punctuation),
            ("elif", Keyword),
            ("x", Function),
            (";", Punctuation),
            ("then", Keyword),
            ("exit", Function),
            ("1", Constant),
            (";", Punctuation),
            ("else", Keyword),
            ("y", Function),
            (";", Punctuation),
            ("fi", Keyword),
        ],
    );
    check(
        "echo if then done",
        &[
            ("echo", Function),
            ("if", String),
            ("then", String),
            ("done", String),
        ],
    );
    check(
        "while read l; do :; done > out",
        &[
            ("while", Keyword),
            ("read", Function),
            ("l", String),
            (";", Punctuation),
            ("do", Keyword),
            (":", Function),
            (";", Punctuation),
            ("done", Keyword),
            (">", Operator),
            ("out", String),
        ],
    );
    check("in x", &[("in", Function), ("x", String)]);
}

#[test]
fn a_for_loop_names_its_variable_then_takes_in_and_its_words() {
    check(
        "for name in a \"b\" $c; do echo $name; done",
        &[
            ("for", Keyword),
            ("in", Keyword),
            ("a", String),
            ("\"b\"", String),
            ("$", Punctuation),
            (";", Punctuation),
            ("do", Keyword),
            ("echo", Function),
            ("$", Punctuation),
            (";", Punctuation),
            ("done", Keyword),
        ],
    );
    check(
        "select x\nin y",
        &[("select", Keyword), ("in", Keyword), ("y", String)],
    );
}

#[test]
fn a_for_loop_without_in_takes_do_on_its_line_or_the_next() {
    check(
        "for x\ndo echo; done",
        &[
            ("for", Keyword),
            ("do", Keyword),
            ("echo", Function),
            (";", Punctuation),
            ("done", Keyword),
        ],
    );
    check(
        "for x do :; done",
        &[
            ("for", Keyword),
            ("do", Keyword),
            (":", Function),
            (";", Punctuation),
            ("done", Keyword),
        ],
    );
}

#[test]
fn a_case_takes_its_subject_in_and_patterns_up_to_their_paren() {
    check(
        "case \"$x\" in\n  a|b) echo one ;;\n  (*) echo other ;&\nesac",
        &[
            ("case", Keyword),
            ("\"", String),
            ("$", Punctuation),
            ("\"", String),
            ("in", Keyword),
            ("a", String),
            ("|", Operator),
            ("b", String),
            (")", Punctuation),
            ("echo", Function),
            ("one", String),
            (";;", Punctuation),
            ("(", Punctuation),
            ("*", String),
            (")", Punctuation),
            ("echo", Function),
            ("other", String),
            (";&", Punctuation),
            ("esac", Keyword),
        ],
    );
}

#[test]
fn a_function_definition_names_a_function_and_opens_a_command_list() {
    check(
        "function greet {\ngreet() { echo hi; }",
        &[
            ("function", Keyword),
            ("greet", Function),
            ("{", Punctuation),
            ("greet", Function),
            ("(", Punctuation),
            (")", Punctuation),
            ("{", Punctuation),
            ("echo", Function),
            ("hi", String),
            (";", Punctuation),
            ("}", Punctuation),
        ],
    );
}

#[test]
fn an_assignment_keeps_its_name_plain_and_the_command_after_it() {
    check(
        "NAME=value FOO+=1 cmd arg 2x=y",
        &[
            ("=", Operator),
            ("value", String),
            ("+=", Operator),
            ("1", Constant),
            ("cmd", Function),
            ("arg", String),
            ("2x=y", String),
        ],
    );
    check(
        "export PATH=\"$HOME/bin\" -n x\nlocal y",
        &[
            ("export", Keyword),
            ("=", Operator),
            ("\"", String),
            ("$", Punctuation),
            ("/bin\"", String),
            ("-n", Constant),
            ("local", Keyword),
        ],
    );
    check("=x", &[("=x", Function)]);
}

#[test]
fn an_array_takes_its_words_as_arguments_up_to_its_own_paren() {
    check(
        "files=(a \"b c\" $d 4 -x if) cmd",
        &[
            ("=", Operator),
            ("(", Punctuation),
            ("a", String),
            ("\"b c\"", String),
            ("$", Punctuation),
            ("4", Constant),
            ("-x", Constant),
            ("if", String),
            (")", Punctuation),
            ("cmd", Function),
        ],
    );
    check(
        "files=($(ls) x) y",
        &[
            ("=", Operator),
            ("(", Punctuation),
            ("$(", Punctuation),
            ("ls", Function),
            (")", Punctuation),
            ("x", String),
            (")", Punctuation),
            ("y", Function),
        ],
    );
    check(
        "declare -a arr=(x) -r",
        &[
            ("declare", Keyword),
            ("-a", Constant),
            ("=", Operator),
            ("(", Punctuation),
            ("x", String),
            (")", Punctuation),
            ("-r", Constant),
        ],
    );
}

#[test]
fn an_array_spans_lines() {
    check(
        "files=(\n  one # first\n  two\n)\nls",
        &[
            ("=", Operator),
            ("(", Punctuation),
            ("one", String),
            ("# first", Comment),
            ("two", String),
            (")", Punctuation),
            ("ls", Function),
        ],
    );
}

#[test]
fn an_assignment_takes_a_subscript_before_its_operator() {
    check(
        "arr[$i$j]=v map[${k[0]}]+=w x[1]y=z",
        &[
            ("[", Punctuation),
            ("$", Punctuation),
            ("$", Punctuation),
            ("]", Punctuation),
            ("=", Operator),
            ("v", String),
            ("[", Punctuation),
            ("${", Punctuation),
            ("}", Punctuation),
            ("]", Punctuation),
            ("+=", Operator),
            ("w", String),
            ("x[1]y=z", Function),
        ],
    );
}

#[test]
fn a_command_substitution_opens_a_command() {
    check(
        "x=$(date +%s) && echo `date -u` z",
        &[
            ("=", Operator),
            ("$(", Punctuation),
            ("date", Function),
            ("+%s", String),
            (")", Punctuation),
            ("&&", Operator),
            ("echo", Function),
            ("`", Punctuation),
            ("date", Function),
            ("-u", Constant),
            ("`", Punctuation),
            ("z", String),
        ],
    );
}

#[test]
fn redirections_are_operators_and_a_descriptor_a_constant() {
    check(
        "cmd 2>&1 >> log < in &> all >| x",
        &[
            ("cmd", Function),
            ("2", Constant),
            (">&", Operator),
            ("1", Constant),
            (">>", Operator),
            ("log", String),
            ("<", Operator),
            ("in", String),
            ("&>", Operator),
            ("all", String),
            (">|", Operator),
            ("x", String),
        ],
    );
}

#[test]
fn a_test_takes_its_operators_as_keywords_and_operators() {
    check(
        "if [[ $a == \"b\" && -n $c ]]; then [ ! -f x ]; fi",
        &[
            ("if", Keyword),
            ("[[", Punctuation),
            ("$", Punctuation),
            ("==", Operator),
            ("\"b\"", String),
            ("&&", Operator),
            ("-n", Keyword),
            ("$", Punctuation),
            ("]]", Punctuation),
            (";", Punctuation),
            ("then", Keyword),
            ("[", Punctuation),
            ("!", Operator),
            ("-f", Keyword),
            ("x", String),
            ("]", Punctuation),
            (";", Punctuation),
            ("fi", Keyword),
        ],
    );
}

#[test]
fn a_bang_negates_a_pipeline() {
    check(
        "! grep x",
        &[("!", Operator), ("grep", Function), ("x", String)],
    );
}
