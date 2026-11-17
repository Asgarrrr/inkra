//! Task gutters (task boxes spec, D1 and D2): a task item whose marker leads its line.

use inkra_text::ByteOffset;

use super::super::parse;
use super::{concealed, styled};
use crate::{Styles, is_active};

/// Each gutter with a task as its text, its checkbox's text and whether it is checked.
fn tasks(text: &str) -> Vec<(&str, &str, bool)> {
    parse(text)
        .gutters
        .iter()
        .filter_map(|gutter| {
            let task = gutter.task.as_ref()?;
            let range = gutter.range.start.get()..gutter.range.end.get();
            let checkbox = task.checkbox.start.get()..task.checkbox.end.get();
            Some((&text[range], &text[checkbox], task.checked))
        })
        .collect()
}

#[test]
fn a_task_item_has_a_gutter_through_its_checkbox_and_one_space() {
    assert_eq!(tasks("- [ ] a"), vec![("- [ ] ", "[ ]", false)]);
    assert_eq!(tasks("- [x] a"), vec![("- [x] ", "[x]", true)]);
    assert_eq!(tasks("- [X] a"), vec![("- [X] ", "[X]", true)]);
    assert_eq!(tasks("* [ ] a"), vec![("* [ ] ", "[ ]", false)]);
    assert_eq!(tasks("1. [ ] a"), vec![("1. [ ] ", "[ ]", false)]);
    assert_eq!(tasks("  - [ ] a"), vec![("  - [ ] ", "[ ]", false)]);
    assert_eq!(tasks("-    [ ] a"), vec![("-    [ ] ", "[ ]", false)]);
    assert_eq!(tasks("- [ ]  a"), vec![("- [ ] ", "[ ]", false)]);
}

#[test]
fn a_quoted_task_shares_its_lines_gutter() {
    assert_eq!(tasks("> - [ ] a"), vec![("> - [ ] ", "[ ]", false)]);
    let gutter = &parse("> - [ ] a").gutters[0];
    assert_eq!(
        (gutter.quotes.clone(), gutter.depth),
        (vec![ByteOffset::new(0)], 1)
    );
    assert_eq!(tasks("> > - [x] a"), vec![("> > - [x] ", "[x]", true)]);
    assert_eq!(
        tasks("> a\n>   - [ ] b"),
        vec![(">   - [ ] ", "[ ]", false)]
    );
}

#[test]
fn a_task_outside_a_quote_has_depth_zero() {
    let gutters = parse("a\n- [ ] b").gutters;
    assert_eq!(gutters.len(), 1);
    assert_eq!((gutters[0].quotes.len(), gutters[0].depth), (0, 0));
}

#[test]
fn a_loose_task_is_decided_past_its_paragraph() {
    assert_eq!(
        tasks("- [ ] a\n\n- [x] b"),
        vec![("- [ ] ", "[ ]", false), ("- [x] ", "[x]", true)]
    );
    assert_eq!(
        concealed("- [ ] a\n\n- [x] b"),
        vec![("- [ ]", "- [ ]", ""), ("- [x]", "- [x]", "")]
    );
}

#[test]
fn a_checkbox_pulldown_cmark_reads_as_text_is_no_task() {
    assert_eq!(tasks("- [ ]a"), vec![]);
    assert_eq!(concealed("- [ ]a"), vec![("-", "-", "•")]);
}

/// The last line reads as if a newline ended it, so a checkbox there is a task with no text.
#[test]
fn a_checkbox_ending_the_text_is_a_task_with_no_text() {
    assert_eq!(tasks("- [ ]"), vec![("- [ ]", "[ ]", false)]);
    assert_eq!(tasks("a\n- [x]"), vec![("- [x]", "[x]", true)]);
    assert_eq!(concealed("- [ ]"), vec![("- [ ]", "- [ ]", "")]);
}

#[test]
fn a_task_with_no_text_ends_its_gutter_at_the_checkbox() {
    assert_eq!(tasks("- [ ]\n"), vec![("- [ ]", "[ ]", false)]);
    assert_eq!(tasks("- [x]\nb"), vec![("- [x]", "[x]", true)]);
}

#[test]
fn an_empty_item_keeps_its_dot() {
    assert_eq!(tasks("-\n"), vec![]);
    assert_eq!(concealed("-\n"), vec![("-", "-", "•")]);
}

#[test]
fn a_task_after_another_items_bullet_keeps_its_dot_and_no_gutter() {
    assert_eq!(tasks("- - [ ] a"), vec![]);
    assert_eq!(parse("- - [ ] a").gutters, vec![]);
    assert_eq!(
        concealed("- - [ ] a"),
        vec![("-", "-", "•"), ("-", "-", "•")]
    );
    assert_eq!(tasks("- > - [ ] a"), vec![]);
}

#[test]
fn a_quoted_line_keeps_its_quote_gutter_under_a_task_that_does_not_lead() {
    let gutters = parse("> - - [ ] a").gutters;
    assert_eq!(gutters.len(), 1);
    assert_eq!(gutters[0].range, ByteOffset::new(0)..ByteOffset::new(2));
    assert_eq!((gutters[0].depth, gutters[0].task.clone()), (1, None));
}

#[test]
fn a_nested_task_on_its_own_line_has_a_gutter() {
    assert_eq!(tasks("- a\n  - [ ] b"), vec![("  - [ ] ", "[ ]", false)]);
    assert_eq!(
        concealed("- a\n  - [ ] b"),
        vec![("-", "-", "•"), ("  - [ ]", "  - [ ]", "")]
    );
}

/// pulldown-cmark starts a tab-indented item at the line ending before it.
#[test]
fn a_tab_indented_nested_item_has_its_marker() {
    assert_eq!(tasks("- a\n\t- [ ] b"), vec![("\t- [ ] ", "[ ]", false)]);
    assert_eq!(tasks("- a\n\n\t- [x] b"), vec![("\t- [x] ", "[x]", true)]);
    assert_eq!(
        concealed("- a\n\t- b"),
        vec![("-", "-", "•"), ("-", "-", "•")]
    );
    assert_eq!(struck("- a\n\t- [x] b"), ["b"]);
    assert_eq!(tasks(">\t- [ ] b"), vec![(">\t- [ ] ", "[ ]", false)]);
    assert_eq!(concealed(">\t- b"), vec![(">", ">", ""), ("-", "-", "•")]);
}

#[test]
fn the_task_conceal_covers_the_task_part_and_hides_an_ordered_number() {
    assert_eq!(concealed("- [ ] a"), vec![("- [ ]", "- [ ]", "")]);
    assert_eq!(concealed("1. [x] a"), vec![("1. [x]", "1. [x]", "")]);
    assert_eq!(concealed("  - [ ] a"), vec![("  - [ ]", "  - [ ]", "")]);
}

#[test]
fn the_quote_part_and_the_task_part_conceal_apart() {
    assert_eq!(
        concealed("> - [ ] a"),
        vec![(">", ">", ""), ("- [ ]", "- [ ]", "")]
    );
    let conceals = parse("> - [ ] a").conceals;
    let caret = |at| ByteOffset::new(at)..ByteOffset::new(at);
    assert!(!is_active(&conceals[0], &caret(0)) && is_active(&conceals[1], &caret(0)));
    assert!(is_active(&conceals[0], &caret(2)) && !is_active(&conceals[1], &caret(2)));
}

#[test]
fn a_caret_in_the_task_part_reveals_it_and_one_at_the_text_start_does_not() {
    let conceal = &parse("- [ ] a").conceals[0];
    let caret = |at| ByteOffset::new(at)..ByteOffset::new(at);
    assert!(!is_active(conceal, &caret(0)));
    assert!(!is_active(conceal, &caret(5)));
    assert!(is_active(conceal, &caret(6)));
}

#[test]
fn a_task_part_keeps_its_gap_style() {
    const Q: Styles = Styles::QUOTE;
    const M: Styles = Styles::MARKER;
    const D: Styles = Styles::DONE;
    assert_eq!(styled("> - [ ] a"), vec![("> - [ ] ", Q | M), ("a", Q)]);
    assert_eq!(styled("- [x] a"), vec![("- [x] ", D | M), ("a", D)]);
    assert_eq!(
        styled("- [x] a\n  b"),
        vec![("- [x] ", D | M), ("a", D), ("  ", D | M), ("b", D)]
    );
}

/// The text `DONE` strikes: its spans that are no marker (D7).
fn struck(text: &str) -> Vec<&str> {
    parse(text)
        .spans
        .iter()
        .filter(|span| span.styles.contains(Styles::DONE) && !span.styles.contains(Styles::MARKER))
        .map(|span| &text[span.range.start.get()..span.range.end.get()])
        .collect()
}

#[test]
fn a_checked_task_strikes_its_own_text_only() {
    assert_eq!(struck("- [x] a"), ["a"]);
    assert_eq!(
        struck(
            "- [X] a
b"
        ),
        ["a", "b"]
    );
    assert_eq!(struck("1. [x] a"), ["a"]);
    assert_eq!(struck("> - [x] a"), ["a"]);
    assert_eq!(struck("- [ ] a"), [""; 0]);
    assert_eq!(struck("- a"), [""; 0]);
}

#[test]
fn a_checked_task_strikes_its_continuation_lines_but_not_their_indent() {
    assert_eq!(struck("- [x] a\n  b"), ["a", "b"]);
    assert_eq!(struck("> - [x] a\n>   b"), ["a", "b"]);
}

#[test]
fn a_loose_checked_task_strikes_every_paragraph() {
    assert_eq!(struck("- [x] a\n\n  b\n\n  c"), ["a", "b", "c"]);
    assert_eq!(struck("- [x] a\n\n  # h"), ["a", "h"]);
    assert_eq!(struck("- [x] a\n\n- [ ] b"), ["a"]);
    assert_eq!(struck("- [ ] a\n\n- [x] b"), ["b"]);
}

#[test]
fn a_checked_task_does_not_strike_a_block_inside_it() {
    assert_eq!(struck("- [x] a\n  - [ ] b"), ["a"]);
    assert_eq!(struck("- [x] a\n  - b"), ["a"]);
    assert_eq!(struck("- [x] a\n\n  > b"), ["a"]);
    assert_eq!(struck("- [x] a\n\n  ```\n  b\n  ```"), ["a"]);
    assert_eq!(struck("- [x] a\n\n        b"), ["a"]);
    assert_eq!(struck("- [ ] a\n  - [x] b"), ["b"]);
}

#[test]
fn a_checked_task_with_no_gutter_strikes_nothing() {
    assert_eq!(struck("- - [x] a"), [""; 0]);
    assert_eq!(struck("- > - [x] a"), [""; 0]);
}
