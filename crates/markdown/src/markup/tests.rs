use proptest::prelude::*;

use super::markup;

fn plain(line: &str) -> String {
    let mut shown = String::new();
    let mut at = 0;
    for range in markup(line, true) {
        shown.push_str(&line[at..range.start.get()]);
        at = range.end.get();
    }
    shown + &line[at..]
}

fn plain_inside(piece: &str) -> String {
    let mut shown = String::new();
    let mut at = 0;
    for range in markup(piece, false) {
        shown.push_str(&piece[at..range.start.get()]);
        at = range.end.get();
    }
    shown + &piece[at..]
}

#[test]
fn plain_text_has_no_markup() {
    assert_eq!(markup("plain text", true), []);
    assert_eq!(markup("", true), []);
}

#[test]
fn a_heading_drops_its_opening_and_closing_sequences() {
    assert_eq!(plain("# Plan"), "Plan");
    assert_eq!(plain("### Other ##"), "Other");
    assert_eq!(plain("#"), "");
}

#[test]
fn quotes_and_list_items_drop_their_markers() {
    assert_eq!(plain("> quote"), "quote");
    assert_eq!(plain("- item"), "item");
    assert_eq!(plain("* item"), "item");
    assert_eq!(plain("+ item"), "item");
    assert_eq!(plain("1. item"), "item");
    assert_eq!(plain("1) item"), "item");
    assert_eq!(plain("- - nested"), "nested");
}

#[test]
fn task_boxes_drop_with_their_item_marker() {
    assert_eq!(plain("- [ ] task"), "task");
    assert_eq!(plain("- [x] done"), "done");
    assert_eq!(plain("> - [x] done"), "done");
}

#[test]
fn emphasis_strong_and_code_drop_their_delimiters() {
    assert_eq!(
        plain("**bold** and _em_ and `code`"),
        "bold and em and code"
    );
    assert_eq!(plain("__strong__ *em*"), "strong em");
    assert_eq!(plain("- **plan** it"), "plan it");
    assert_eq!(plain("# A **bold** title"), "A bold title");
}

#[test]
fn an_underscore_inside_a_word_is_not_emphasis() {
    assert_eq!(markup("snake_case_name", true), []);
    assert_eq!(plain("a snake_case_name here"), "a snake_case_name here");
}

#[test]
fn a_line_of_markers_alone_drops_whole() {
    assert_eq!(plain("---"), "");
    assert_eq!(plain("***"), "");
    assert_eq!(plain("- "), "");
    assert_eq!(plain(">"), "");
}

#[test]
fn other_syntax_is_kept() {
    assert_eq!(plain("[a **b**](url)"), "[a b](url)");
    assert_eq!(plain("~~gone~~"), "~~gone~~");
    assert_eq!(plain("[[wiki]]"), "[[wiki]]");
    assert_eq!(plain("```rust"), "```rust");
    assert_eq!(plain("<b>x</b>"), "<b>x</b>");
    assert_eq!(plain(r"\*not em\*"), r"\*not em\*");
}

#[test]
fn inside_a_line_no_block_marker_leads() {
    assert_eq!(plain_inside("- c and **x**"), "- c and x");
    assert_eq!(plain_inside("# not a heading"), "# not a heading");
    assert_eq!(plain_inside("    **indented**"), "    indented");
    assert_eq!(plain_inside("1. two"), "1. two");
}

#[test]
fn ranges_are_sorted_disjoint_and_never_empty() {
    for line in [
        "> - [ ] **a** _b_ `c` ##",
        "# x #",
        "- ",
        "*a* **b** ***c***",
    ] {
        for at_line_start in [true, false] {
            let ranges = markup(line, at_line_start);
            assert!(
                ranges.iter().all(|range| range.start < range.end),
                "{line:?}"
            );
            assert!(
                ranges.windows(2).all(|pair| pair[0].end <= pair[1].start),
                "{line:?}"
            );
            assert!(
                ranges
                    .last()
                    .is_none_or(|range| range.end.get() <= line.len())
            );
        }
    }
}

proptest! {
    #[test]
    fn ranges_hold_on_any_line(
        line in "[-*_`#>+.)\\[\\] x1\u{a0}é~|<]{0,24}",
        at_line_start: bool,
    ) {
        let ranges = markup(&line, at_line_start);
        for range in &ranges {
            prop_assert!(range.start < range.end);
            prop_assert!(range.end.get() <= line.len());
            prop_assert!(line.is_char_boundary(range.start.get()) && line.is_char_boundary(range.end.get()));
        }
        prop_assert!(ranges.windows(2).all(|pair| pair[0].end < pair[1].start));
    }
}
