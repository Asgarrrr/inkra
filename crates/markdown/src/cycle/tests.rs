use inkra_text::{ByteOffset, Hunk};
use proptest::prelude::*;

use super::cycle_tasks;
use crate::parse;

fn apply(text: &str, hunks: &[Hunk]) -> String {
    let mut out = text.to_owned();
    for hunk in hunks.iter().rev() {
        out.replace_range(hunk.range.start.get()..hunk.range.end.get(), &hunk.text);
    }
    out
}

/// `text` after `⌘L` with the caret at `caret`.
fn cycled(text: &str, caret: usize) -> String {
    let at = ByteOffset::new(caret);
    apply(text, &cycle_tasks(text, at..at))
}

/// `text`'s one line cycled, the same from a caret at its start as at its end.
fn cycled_line(text: &str) -> String {
    let from_start = cycled(text, 0);
    assert_eq!(cycled(text, text.len()), from_start, "{text:?}");
    from_start
}

#[test]
fn an_unchecked_task_is_checked() {
    for (line, want) in [("- [ ] a", "- [x] a"), ("1. [ ] a", "1. [x] a")] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

#[test]
fn a_checked_task_is_cleared_in_either_case() {
    for line in ["- [x] a", "- [X] a"] {
        assert_eq!(cycled_line(line), "- [ ] a", "{line:?}");
    }
}

#[test]
fn a_bullet_gains_a_checkbox_after_its_space() {
    for (line, want) in [
        ("- a", "- [ ] a"),
        ("* a", "* [ ] a"),
        ("+ a", "+ [ ] a"),
        ("1. a", "1. [ ] a"),
        ("3) a", "3) [ ] a"),
        ("123456789. a", "123456789. [ ] a"),
        ("-\ta", "-\t[ ] a"),
        ("-  a", "- [ ]  a"),
        ("- ", "- [ ] "),
        ("- - [ ] a", "- [ ] - [ ] a"),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

#[test]
fn a_plain_line_becomes_a_task_after_its_quotes_and_spaces() {
    for (line, want) in [
        ("a", "- [ ] a"),
        ("> a", "> - [ ] a"),
        ("  a", "  - [ ] a"),
        ("> > a", "> > - [ ] a"),
        (">a", ">- [ ] a"),
        ("", "- [ ] "),
        ("  ", "  - [ ] "),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

/// No bullet without a space or tab after it, and no number past nine digits (`CommonMark` 5.2).
#[test]
fn a_marker_without_its_space_is_plain_text() {
    for (line, want) in [
        ("-a", "- [ ] -a"),
        ("1.a", "- [ ] 1.a"),
        ("*a*", "- [ ] *a*"),
        ("1234567890. a", "- [ ] 1234567890. a"),
        ("a. b", "- [ ] a. b"),
        ("(1) a", "- [ ] (1) a"),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

#[test]
fn a_task_reads_past_its_quotes_and_the_spaces_after_its_bullet() {
    for (line, want) in [
        ("-    [ ] a", "-    [x] a"),
        ("> 1. [x] a", "> 1. [ ] a"),
        ("  - [X]", "  - [ ]"),
        ("- [y] a", "- [ ] [y] a"),
        ("- [ x] a", "- [ ] [ x] a"),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

#[test]
fn only_the_carets_line_changes() {
    let text = "a\n- b\n- [ ] c\n";
    assert_eq!(
        cycled(text, 1),
        "- [ ] a\n- b\n- [ ] c\n",
        "at a line's end"
    );
    assert_eq!(
        cycled(text, 2),
        "a\n- [ ] b\n- [ ] c\n",
        "at a line's start"
    );
    assert_eq!(cycled(text, 13), "a\n- b\n- [x] c\n");
    assert_eq!(
        cycled(text, 14),
        "a\n- b\n- [ ] c\n- [ ] ",
        "on the empty last line"
    );
}

/// Every line of a code block, the front matter or a table, from its first line's start; a fence
/// in a list item starts past it.
#[test]
fn a_code_block_the_front_matter_and_a_table_are_skipped() {
    for (text, from) in [
        ("```\na\n\n```\n", 0),
        ("```\na", 0),
        ("- a\n\n  ```\n  b\n  ```\n", 5),
        ("    a\n\n    b\n", 0),
        ("---\na: b\n---\n", 0),
        ("| a |\n|---|\n| b |\n", 0),
    ] {
        for caret in (from..text.len()).filter(|&at| text.is_char_boundary(at)) {
            assert_eq!(cycled(text, caret), text, "{text:?} at {caret}");
        }
    }
}

#[test]
fn the_line_after_a_skipped_block_cycles() {
    for (text, caret, want) in [
        (
            "| a |\n|---|\n| b |\n\nc",
            18,
            "| a |\n|---|\n| b |\n- [ ] \nc",
        ),
        ("```\na\n```\nb", 10, "```\na\n```\n- [ ] b"),
        ("---\na: b\n---\nc", 13, "---\na: b\n---\n- [ ] c"),
        (
            "- a\n\n  ```\n  b\n  ```\nc",
            0,
            "- [ ] a\n\n  ```\n  b\n  ```\nc",
        ),
    ] {
        assert_eq!(cycled(text, caret), want, "{text:?} at {caret}");
    }
}

/// One line of a generated note: a prefix and a body, each starting a new block after a blank
/// line, so the parse reads every line alone.
fn line() -> impl Strategy<Value = (String, bool)> {
    let prefix = prop::sample::select(vec!["", "> ", "  ", "> > "]);
    let marker = prop::sample::select(vec![
        "", "- ", "* ", "+ ", "1. ", "7) ", "- [ ] ", "- [x] ", "- [X] ", "1. [ ] ",
    ]);
    let body = (marker, "[a-zé]{1,4}").prop_map(|(marker, word)| format!("{marker}{word}"));
    (prefix, prop_oneof![body, Just(String::new())])
        .prop_map(|(prefix, body)| (format!("{prefix}{body}"), false))
}

/// A block the cycle skips, on every line.
fn skipped() -> impl Strategy<Value = (String, bool)> {
    prop::sample::select(vec![
        "```\na\n```",
        "~~~ rust\n- a\n~~~",
        "| a |\n|---|\n| - b |",
    ])
    .prop_map(|block| (block.to_owned(), true))
}

/// Whether the parse boxes the line starting at `start`, and checked.
fn task_at(text: &str, start: usize) -> Option<bool> {
    parse(text)
        .gutters
        .into_iter()
        .find(|gutter| gutter.range.start.get() == start)
        .and_then(|gutter| gutter.task)
        .map(|task| task.checked)
}

proptest! {
    /// The caret's line reparses as the task state after its own, every other line untouched; a
    /// line of a skipped block, front matter included, edits nothing.
    #[test]
    fn a_cycled_line_reparses_as_the_next_state(
        front in any::<bool>(),
        blocks in prop::collection::vec(prop_oneof![line(), skipped()], 1..5),
        pick in any::<prop::sample::Index>(),
        column in any::<prop::sample::Index>(),
    ) {
        let front = front.then_some(("---\na: b\n---".to_owned(), true));
        // Each line's start and whether a skipped block holds it; a blank line sits between blocks.
        let mut text = String::new();
        let mut lines = Vec::new();
        for (index, (block, skipped)) in front.into_iter().chain(blocks).enumerate() {
            if index > 0 {
                lines.push((text.len(), false));
                text.push('\n');
            }
            for line in block.split('\n') {
                lines.push((text.len(), skipped));
                text.push_str(line);
                text.push('\n');
            }
        }
        let (start, skipped) = lines[pick.index(lines.len())];
        let end = start + text[start..].find('\n').expect("every line ends in a newline");
        let carets: Vec<usize> = (start..=end).filter(|&at| text.is_char_boundary(at)).collect();
        let caret = ByteOffset::new(carets[column.index(carets.len())]);

        let hunks = cycle_tasks(&text, caret..caret);
        if skipped {
            prop_assert!(hunks.is_empty(), "{:?} at {:?}", text, caret);
            return Ok(());
        }
        prop_assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        prop_assert!(start <= hunk.range.start.get() && hunk.range.end.get() <= end);
        let after = apply(&text, &hunks);
        let next = task_at(&text, start) == Some(false);
        prop_assert_eq!(task_at(&after, start), Some(next), "{:?} became {:?}", text, after);
        let delta = after.len() - text.len();
        prop_assert_eq!(&after[..start], &text[..start]);
        prop_assert_eq!(&after[end + delta..], &text[end..]);
    }
}
