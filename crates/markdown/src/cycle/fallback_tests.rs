//! `⌘L` from the parse's structure: the fallback on a line where no item opens, the skipped ATX
//! headings, and the passes a batch runs (task boxes spec, D6, T6a).

use inkra_text::ByteOffset;

use super::tests::apply;
use super::{FALLBACKS, PARSES, cycle_tasks};
use crate::parse;

fn selected(text: &str, start: usize, end: usize) -> String {
    let range = ByteOffset::new(start)..ByteOffset::new(end);
    apply(text, &cycle_tasks(text, &parse(text), range))
}

/// The copy parses and fallback passes one `⌘L` runs.
fn passes(text: &str, start: usize, end: usize) -> (usize, usize) {
    PARSES.set(0);
    FALLBACKS.set(0);
    selected(text, start, end);
    (PARSES.get(), FALLBACKS.get())
}

/// A setext underline and an empty item after a paragraph open no item, so pulldown-cmark reads
/// the line alone for its marker.
#[test]
fn a_marker_the_parse_reads_as_no_item_takes_the_checkbox_after_it() {
    for text in ["a\n- ", "a\n-"] {
        assert_eq!(selected(text, 2, 2), "a\n- [ ] ", "{text:?}");
    }
}

/// The pass starts its tab stops where the line's do, so the tab spans two columns, not three.
#[test]
fn the_fallback_counts_columns_from_the_lines_start() {
    assert_eq!(selected("a\n -\t  ", 2, 2), "a\n -\t  [ ] ");
    assert_eq!(selected("a\n  -\t  ", 2, 2), "a\n  -\t  [ ] ");
}

/// The fallback takes the innermost item it reads, as a line where an item opens does: a new item
/// above lets each of these lines open its items (D6).
#[test]
fn the_fallback_takes_the_innermost_item() {
    let texts = ["a\n\t- - b", "a\n\t* - b", "a\n2. 3. b"];
    let all = texts.map(|text| selected(text, 0, text.len()));
    assert_eq!(
        all,
        [
            "- [ ] a\n\t- - [ ] b",
            "- [ ] a\n\t* - [ ] b",
            "- [ ] a\n2. 3. [ ] b"
        ]
    );
}

/// An opened item's blanks count in columns from the line's start, through the openers before its
/// marker, as the fallback's do (D6).
#[test]
fn a_content_start_counts_columns_from_the_lines_start() {
    for (text, caret, want) in [
        ("> -\t  ", 0, "> -\t  [ ] "),
        ("- a\n\n  -\t  ", 6, "- a\n\n  -\t  [ ] "),
        ("- a\n\n -\t  ", 6, "- a\n\n -\t  [ ] "),
    ] {
        assert_eq!(selected(text, caret, caret), want, "{text:?}");
    }
}

/// pulldown-cmark reads a thematic break before a list marker, so no item gives a content start.
#[test]
fn a_thematic_break_becomes_a_task_as_plain_text() {
    assert_eq!(selected("- - -", 0, 0), "- [ ] - - -");
    assert_eq!(selected("* **", 0, 0), "- [ ] * **");
}

/// After a checkbox the heading would read as the task's text.
#[test]
fn an_atx_heading_is_skipped() {
    for text in ["# a", "- # a", "> # a", "## a\nb"] {
        for caret in 0..=text.find('\n').unwrap_or(text.len()) {
            assert_eq!(selected(text, caret, caret), text, "{text:?} at {caret}");
        }
    }
    assert_eq!(selected("# a\nb", 0, 5), "# a\n- [ ] b");
}

/// `a\n- ` is a setext heading, and the `- ` typed under a line must still cycle.
#[test]
fn a_setext_heading_is_not_skipped() {
    assert_eq!(selected("a\n===", 0, 0), "- [ ] a\n===");
}

/// A skipped range ends before its last `\n`, so the empty last line of an open fence starts at
/// its end, and is skipped.
#[test]
fn a_skipped_ranges_end_is_included() {
    assert_eq!(passes("```\n\n", 4, 4), (0, 0));
    assert_eq!(selected("```\n\n", 4, 4), "```\n\n");
}

/// A flip lands as it stands, so a batch of flips only parses nothing.
#[test]
fn a_batch_of_flips_runs_no_pass() {
    let text = "- [ ] a\n1. [x] b\n> - [ ] c";
    assert_eq!(passes(text, 0, text.len()), (0, 0));
    assert_eq!(
        selected(text, 0, text.len()),
        "- [x] a\n1. [x] b\n> - [x] c"
    );
}

#[test]
fn a_batch_of_plain_lines_runs_one_parse_and_one_fallback_per_line() {
    assert_eq!(passes("a\nb", 0, 3), (1, 2));
}

#[test]
fn a_batch_of_items_runs_one_parse_and_no_fallback() {
    assert_eq!(passes("- a\n- b", 0, 7), (1, 0));
}

/// A flip beside a line that fails is still judged by the parse: the failing line drops out alone.
#[test]
fn a_flip_beside_a_failing_line_keeps_its_own_splice() {
    assert_eq!(selected("- [ ] a\n- > b", 0, 13), "- [x] a\n- > b");
}
