use inkra_text::{ByteOffset, Hunk};
use proptest::prelude::*;

use pulldown_cmark::{Event, Parser, Tag};

use super::cycle_tasks;
use crate::cmark::options;
use crate::parse;
use crate::spans::line_start;

pub(super) fn apply(text: &str, hunks: &[Hunk]) -> String {
    let mut out = text.to_owned();
    for hunk in hunks.iter().rev() {
        out.replace_range(hunk.range.start.get()..hunk.range.end.get(), &hunk.text);
    }
    out
}

/// `text` after `⌘L` with the caret at `caret`.
fn cycled(text: &str, caret: usize) -> String {
    let at = ByteOffset::new(caret);
    apply(text, &cycle_tasks(text, &parse(text), at..at))
}

/// `text`'s one line cycled, the same from a caret at its start as at its end.
fn cycled_line(text: &str) -> String {
    let from_start = cycled(text, 0);
    assert_eq!(cycled(text, text.len()), from_start, "{text:?}");
    from_start
}

#[test]
fn an_unchecked_task_is_checked() {
    for (line, want) in [
        ("- [ ] a", "- [x] a"),
        ("1. [ ] a", "1. [x] a"),
        ("- - [ ] a", "- - [x] a"),
        ("1. - [ ] a", "1. - [x] a"),
    ] {
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
        ("- [ ]a", "- [ ] [ ]a"),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

/// The checkbox goes after the innermost marker, so it makes a task of the inner item (D10).
#[test]
fn a_nested_bullet_gains_a_checkbox_after_the_innermost_marker() {
    for (line, want) in [
        ("- 1. a", "- 1. [ ] a"),
        ("- - a", "- - [ ] a"),
        ("1. - a", "1. - [ ] a"),
        ("> - * a", "> - * [ ] a"),
        ("- -", "- - [ ] "),
        ("-  - a", "-  - [ ] a"),
        ("-   1. a", "-   1. [ ] a"),
        ("-\t- a", "-\t- [ ] a"),
        ("1.    - a", "1.    - [ ] a"),
    ] {
        assert_eq!(cycled_line(line), want, "{line:?}");
    }
}

/// No candidate keeps the quote, so the line is left as it is rather than turn `>` to text.
#[test]
fn a_line_whose_markup_no_candidate_keeps_edits_nothing() {
    for line in ["- > a", "1. > a", "- - > a"] {
        assert_eq!(cycled_line(line), line, "{line:?}");
    }
}

/// Its checkbox is no box's, so a second one would make two on the line (D10).
#[test]
fn a_task_with_no_box_is_skipped() {
    for text in [
        "- > - [ ] a",
        "- > - [x] a\n",
        "- > - [ ]",
        "a\n- > - [ ] b",
    ] {
        for caret in (0..=text.len()).filter(|&at| text.is_char_boundary(at)) {
            let line = text[..caret].rfind('\n').map_or(0, |at| at + 1);
            if text[line..].starts_with('-') {
                assert_eq!(cycled(text, caret), text, "{text:?} at {caret}");
            }
        }
    }
    let text = "a\n- > - [ ] b";
    let all = ByteOffset::new(0)..ByteOffset::new(text.len());
    assert_eq!(
        apply(text, &cycle_tasks(text, &parse(text), all)),
        "- [ ] a\n- > - [ ] b"
    );
}

/// An empty item keeps its `•` (D1), so it is a bullet too.
#[test]
fn an_empty_item_gains_a_checkbox_after_a_space() {
    for (line, want) in [("-", "- [ ] "), ("1.", "1. [ ] "), ("*", "* [ ] ")] {
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
        ("  - [X] b", "  - [ ] b"),
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

/// Every line of a code block, the front matter, a table or an HTML block, from its first line's
/// start; a fence in a list item starts past it.
#[test]
fn a_code_block_the_front_matter_a_table_and_html_are_skipped() {
    for (text, from) in [
        ("```\na\n\n```\n", 0),
        ("```\na", 0),
        ("- a\n\n  ```\n  b\n  ```\n", 5),
        ("    a\n\n    b\n", 0),
        ("---\na: b\n---\n", 0),
        ("| a |\n|---|\n| b |\n", 0),
        ("<div>\n- [ ] a\n</div>", 0),
        ("<!--\na\n-->", 0),
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

/// A line no marker makes a task, as the parse reads it, is left as it is.
#[test]
fn a_line_no_marker_makes_a_task_edits_nothing() {
    for (text, caret) in [
        ("a\n    - [ ] b", 2),
        ("a\n    b", 2),
        ("- a\n      - [ ] b", 4),
        ("```\na\n", 6),
    ] {
        assert_eq!(cycled(text, caret), text, "{text:?} at {caret}");
    }
}

#[test]
fn a_tab_indented_nested_item_cycles() {
    assert_eq!(cycled("- a\n\t- [ ] b", 4), "- a\n\t- [x] b");
    assert_eq!(cycled("- a\n\t- [x] b", 4), "- a\n\t- [ ] b");
    assert_eq!(cycled("- a\n\t- b", 4), "- a\n\t- [ ] b");
}

/// A checkbox ending the text is a task, so it flips instead of gaining a second one.
#[test]
fn a_task_ending_the_text_flips() {
    let text = "- [ ] test\n- [ ]\n- [ ]";
    assert_eq!(cycled(text, text.len()), "- [ ] test\n- [ ]\n- [x]");
}

/// A lazy `2.` is paragraph text, so it takes a bullet of its own.
#[test]
fn a_lazy_number_becomes_a_task_as_plain_text() {
    assert_eq!(cycled("a\n2. b", 2), "a\n- [ ] 2. b");
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
        // A `<div>` block runs on through the blank line after it, which then cannot cycle.
        "<!--\n- a\n-->",
    ])
    .prop_map(|block| (block.to_owned(), true))
}

/// Whether the parse boxes the line starting at `start`, and checked.
pub(super) fn task_at(text: &str, start: usize) -> Option<bool> {
    parse(text)
        .gutters
        .into_iter()
        .find(|gutter| gutter.range.start.get() == start)
        .and_then(|gutter| gutter.task)
        .map(|task| task.checked)
}

/// A caret on the line `start..end` of `text`, picked by `column`.
fn caret_on(text: &str, start: usize, end: usize, column: prop::sample::Index) -> ByteOffset {
    let carets: Vec<usize> = (start..=end)
        .filter(|&at| text.is_char_boundary(at))
        .collect();
    ByteOffset::new(carets[column.index(carets.len())])
}

/// `⌘L` from `caret` on the line `start..end`: a splice leaves the line a task in the state after
/// its own, as the parse reads both, and every other line untouched. Returns the splices.
fn cycle_checked(
    text: &str,
    start: usize,
    end: usize,
    caret: ByteOffset,
) -> Result<Vec<Hunk>, TestCaseError> {
    let hunks = cycle_tasks(text, &parse(text), caret..caret);
    prop_assert!(hunks.len() <= 1);
    let before = task_at(text, start);
    if let [hunk] = hunks.as_slice() {
        prop_assert!(start <= hunk.range.start.get() && hunk.range.end.get() <= end);
        let after = apply(text, &hunks);
        let next = before == Some(false);
        prop_assert_eq!(
            task_at(&after, start),
            Some(next),
            "{:?} became {:?}",
            text,
            after
        );
        prop_assert_eq!(
            task_markers(&after, start),
            1,
            "{:?} became {:?}",
            text,
            after
        );
        prop_assert!(keeps_markup(text, &after), "{:?} became {:?}", text, after);
        let delta = after.len() - text.len();
        prop_assert_eq!(&after[..start], &text[..start]);
        prop_assert_eq!(&after[end + delta..], &text[end..]);
    } else {
        prop_assert!(
            before.is_none(),
            "a task in {:?} at {:?} did not flip",
            text,
            caret
        );
    }
    Ok(hunks)
}

/// The checkboxes pulldown-cmark reads on the line at `start`, the text read as `parse` reads it.
pub(super) fn task_markers(text: &str, start: usize) -> usize {
    let text = if text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    };
    Parser::new_ext(&text, options())
        .into_offset_iter()
        .filter(|(event, range)| {
            matches!(event, Event::TaskListMarker(_)) && line_start(&text, range.start) == start
        })
        .count()
}

/// The list items and block quotes that open on each line, by line index, the text read as
/// `parse` reads it: an item at its marker, a quote at its first `>`.
fn opens_per_line(text: &str) -> Vec<(usize, usize)> {
    let terminated = format!("{text}\n");
    let mut lines = vec![(0, 0); text.matches('\n').count() + 2];
    for (event, range) in Parser::new_ext(&terminated, options()).into_offset_iter() {
        let skip: &[u8] = match event {
            Event::Start(Tag::Item) => b" \t\n>",
            Event::Start(Tag::BlockQuote(_)) => b" \t\n",
            _ => continue,
        };
        let bytes = &terminated.as_bytes()[range.start..];
        let at = range.start + bytes.iter().take_while(|byte| skip.contains(byte)).count();
        let line = &mut lines[terminated[..at].matches('\n').count()];
        match event {
            Event::Start(Tag::Item) => line.0 += 1,
            _ => line.1 += 1,
        }
    }
    lines
}

/// Whether no line of `after`, `text` spliced within its lines, opens fewer items or quotes.
pub(super) fn keeps_markup(text: &str, after: &str) -> bool {
    let (before, now) = (opens_per_line(text), opens_per_line(after));
    before
        .iter()
        .zip(&now)
        .all(|(was, is)| is.0 >= was.0 && is.1 >= was.1)
}

/// Whether a code block, the front matter, a table or an HTML block holds a byte of `start..=end`.
pub(super) fn in_block(text: &str, start: usize, end: usize) -> bool {
    Parser::new_ext(text, options())
        .into_offset_iter()
        .any(|(event, range)| {
            matches!(
                event,
                Event::Start(
                    Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_) | Tag::HtmlBlock
                )
            ) && range.start <= end
                && start < range.end
        })
}

/// A free-form line, mostly markers, or one that opens a block.
pub(super) fn free_line() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[ \t>*+129.)\\[\\]xXa-]{0,8}",
        1 => prop::sample::select(vec!["```", "<div>", "---", "| a |"]).prop_map(str::to_owned),
    ]
}

proptest! {
    /// Every line outside a skipped block, each one block apart, cycles; front matter included,
    /// no line of a skipped block edits.
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
        let caret = caret_on(&text, start, end, column);
        let hunks = cycle_checked(&text, start, end, caret)?;
        prop_assert_eq!(hunks.is_empty(), skipped, "{:?} at {:?}", text, caret);
    }

    /// On any lines: a cycled line is a task in the next state; a line left as it is is no task,
    /// and `- [ ] ` after its quotes and spaces makes none or turns markup to text, unless a
    /// skipped block holds it or it holds a checkbox with no box.
    #[test]
    fn a_free_form_line_cycles_or_cannot_be_a_task(
        lines in prop::collection::vec(free_line(), 1..6),
        newline in any::<bool>(),
        pick in any::<prop::sample::Index>(),
        column in any::<prop::sample::Index>(),
    ) {
        let text = lines.join("\n") + if newline { "\n" } else { "" };
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let start = starts[pick.index(starts.len())];
        let end = text[start..].find('\n').map_or(text.len(), |len| start + len);
        let caret = caret_on(&text, start, end, column);
        let edited = !cycle_checked(&text, start, end, caret)?.is_empty();
        if !edited && !in_block(&text, start, end) && task_markers(&text, start) == 0 {
            let lead = text[start..end]
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t' | b'>'))
                .count();
            let mut plain = text.clone();
            plain.insert_str(start + lead, "- [ ] ");
            prop_assert!(
                task_at(&plain, start).is_none() || !keeps_markup(&text, &plain),
                "{:?} at {:?}",
                text,
                caret
            );
        }
    }
}
