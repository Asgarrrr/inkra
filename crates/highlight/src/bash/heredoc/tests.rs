use inkra_text::ByteOffset;

use crate::Language::Bash;
use crate::TokenClass::{self, Function, Operator, Punctuation, String};
use crate::fixtures::{expect, r};
use crate::highlight;

#[track_caller]
fn check(text: &str, expected: &[(&str, TokenClass)]) {
    expect(Bash, text, expected);
}

#[test]
fn an_empty_quoted_tag_ends_at_an_empty_line() {
    check(
        "cat <<''\nbody $x\n\necho after",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("''", String),
            ("body $x", String),
            ("echo", Function),
            ("after", String),
        ],
    );
}

#[test]
fn two_heredocs_on_one_line_read_both_bodies_in_order() {
    check(
        "cat <<A <<B\nfirst\nB\nA\nsecond\nA\nB\nls",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("A", String),
            ("<<", Operator),
            ("B", String),
            ("first", String),
            ("B", String),
            ("A", String),
            ("second", String),
            ("A", String),
            ("B", String),
            ("ls", Function),
        ],
    );
}

#[test]
fn a_third_heredoc_is_not_queued_and_its_body_lexes_as_commands() {
    check(
        "cat <<A <<B <<C\na\nA\nb\nB\nc\nC",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("A", String),
            ("<<", Operator),
            ("B", String),
            ("<<", Operator),
            ("C", String),
            ("a", String),
            ("A", String),
            ("b", String),
            ("B", String),
            ("c", Function),
            ("C", Function),
        ],
    );
}

#[test]
fn a_dash_strips_the_tabs_before_the_body_and_the_tag() {
    check(
        "cat <<-EOF\n\t\tbody\n\tEOF\nls",
        &[
            ("cat", Function),
            ("<<-", Operator),
            ("EOF", String),
            ("body", String),
            ("EOF", String),
            ("ls", Function),
        ],
    );
}

#[test]
fn without_a_dash_a_tab_indented_tag_is_body() {
    check(
        "cat <<EOF\n\tEOF\nEOF\nls",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("EOF", String),
            ("\tEOF", String),
            ("EOF", String),
            ("ls", Function),
        ],
    );
}

#[test]
fn the_tag_must_match_the_whole_line() {
    check(
        "cat <<EOF\nEOF \nEOFX\n\nEOF\nls",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("EOF", String),
            ("EOF ", String),
            ("EOFX", String),
            ("EOF", String),
            ("ls", Function),
        ],
    );
}

#[test]
fn a_quoted_tag_is_its_text_without_quotes_and_its_body_expands_nothing() {
    for tag in ["'EOF'", "\"EOF\"", "\\EOF", "E\"O\"F"] {
        let text = format!("cat <<{tag}\n$HOME\nEOF\nls");
        check(
            &text,
            &[
                ("cat", Function),
                ("<<", Operator),
                (tag, String),
                ("$HOME", String),
                ("EOF", String),
                ("ls", Function),
            ],
        );
    }
}

#[test]
fn an_unquoted_tag_body_expands_parameters() {
    check(
        "cat <<EOF\nhi $USER ${x} \\$y\nEOF",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("EOF", String),
            ("hi ", String),
            ("$", Punctuation),
            (" ", String),
            ("${", Punctuation),
            ("}", Punctuation),
            (" \\$y", String),
            ("EOF", String),
        ],
    );
}

#[test]
fn the_rest_of_the_command_line_lexes_before_the_body() {
    check(
        "cat <<EOF | grep x\nbody\nEOF",
        &[
            ("cat", Function),
            ("<<", Operator),
            ("EOF", String),
            ("|", Operator),
            ("grep", Function),
            ("x", String),
            ("body", String),
            ("EOF", String),
        ],
    );
}

#[test]
fn an_operator_with_no_tag_queues_nothing() {
    check(
        "cat <<\nls",
        &[("cat", Function), ("<<", Operator), ("ls", Function)],
    );
}

#[test]
fn a_here_string_queues_no_body() {
    check(
        "cat <<< \"x\"\nls",
        &[
            ("cat", Function),
            ("<<<", Operator),
            ("\"x\"", String),
            ("ls", Function),
        ],
    );
}

#[test]
fn highlight_carries_the_queue_to_the_next_lines_at_their_offsets() {
    let mut out = Vec::new();
    let at = ByteOffset::new;
    highlight(
        Bash,
        [
            (at(0), "cat <<E"),
            (at(10), "x"),
            (at(20), "E"),
            (at(30), "ls"),
        ]
        .into_iter(),
        &mut out,
    );
    assert_eq!(
        out,
        [
            (r(0, 3), Function),
            (r(4, 6), Operator),
            (r(6, 7), String),
            (r(10, 11), String),
            (r(20, 21), String),
            (r(30, 32), Function),
        ]
    );
}
