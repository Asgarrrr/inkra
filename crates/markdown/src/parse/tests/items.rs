//! Item parts (task boxes spec, D1, D2, D11): a gutter item's marker line shows its markers in
//! slots, nested by level, and its numbers take their list's width key.

use inkra_text::ByteOffset;

use super::super::parse;
use super::concealed;
use crate::gutters::gutter_conceals;
use crate::{Gutter, ItemMarker, ItemPart, WidthKey, is_active};

/// A line's item part: its text, its blank slots, its marks as they paint, and whether it ends in
/// a box.
type Part = (String, usize, Vec<String>, bool);

fn part(text: &str, gutter: &Gutter) -> Option<Part> {
    let item = gutter.item.as_ref()?;
    let source = &text[item.start.get()..gutter.range.end.get()];
    let marks = item
        .marks(source)
        .map(|mark| match mark {
            ItemMarker::Bullet => "•".to_owned(),
            ItemMarker::Number(range) => text[range.start.get()..range.end.get()].to_owned(),
        })
        .collect();
    Some((source.to_owned(), item.indent, marks, item.task.is_some()))
}

/// The item part of line `line` of `text`.
fn line_part(text: &str, line: usize) -> Option<Part> {
    let start = text.split('\n').take(line).map(|line| line.len() + 1).sum();
    let gutter = parse(text)
        .gutters
        .into_iter()
        .find(|gutter| gutter.range.start.get() == start)?;
    part(text, &gutter)
}

fn expect(source: &str, indent: usize, marks: &[&str], boxed: bool) -> Part {
    let marks = marks.iter().map(|&mark| mark.to_owned()).collect();
    (source.to_owned(), indent, marks, boxed)
}

#[test]
fn a_marker_line_shows_its_markers_in_slots() {
    for (text, line, source, indent, marks, boxed) in [
        ("- a", 0, "- ", 0, &["•"][..], false),
        ("1. a", 0, "1. ", 0, &["1."], false),
        ("- [ ] a", 0, "- [ ] ", 0, &[], true),
        ("1. [ ] a", 0, "1. [ ] ", 0, &["1."], true),
        ("- - a", 0, "- - ", 0, &["•", "•"], false),
        ("-   a", 0, "- ", 0, &["•"], false),
        ("- a\n  - b", 1, "  - ", 1, &["•"], false),
        ("1. [ ] a\n   - b", 1, "   - ", 2, &["•"], false),
        ("- > - a", 0, "- ", 0, &["•"], false),
        ("* a", 0, "* ", 0, &["•"], false),
        ("+ a", 0, "+ ", 0, &["•"], false),
        ("12) a", 0, "12) ", 0, &["12)"], false),
        ("- 1. [ ] a", 0, "- 1. [ ] ", 0, &["•", "1."], true),
        ("-\n", 0, "-", 0, &["•"], false),
    ] {
        assert_eq!(
            line_part(text, line),
            Some(expect(source, indent, marks, boxed)),
            "{text:?}"
        );
    }
}

/// Nesting counts levels, not blanks (D11).
#[test]
fn a_child_takes_one_blank_slot_per_column_of_its_holders() {
    for text in ["- a\n  - b", "- a\n    - b", "- a\n\t- b"] {
        let (_, indent, marks, _) = line_part(text, 1).unwrap();
        assert_eq!((indent, marks), (1, vec!["•".to_owned()]), "{text:?}");
    }
    assert_eq!(line_part("- a\n  - b\n    - c", 2).unwrap().1, 2);
    assert_eq!(line_part("1. a\n   - b", 1).unwrap().1, 1);
}

#[test]
fn a_quoted_item_part_starts_past_the_quote_part() {
    let text = "> - a";
    let gutter = &parse(text).gutters[0];
    let quote = &text[..gutter.quote_end().get()];
    assert_eq!(quote, "> ");
    assert_eq!(part(text, gutter), Some(expect("- ", 0, &["•"], false)));
}

/// Each line's width keys, in slot order.
fn numbers(text: &str) -> Vec<Vec<WidthKey>> {
    let markdown = parse(text);
    let keys = |item: &ItemPart| markdown.width_keys[item.numbers.clone()].to_vec();
    markdown
        .gutters
        .iter()
        .map(|gutter| gutter.item.as_ref().map(keys).unwrap_or_default())
        .collect()
}

const fn key(digits: u8, delimiter: u8) -> WidthKey {
    WidthKey { digits, delimiter }
}

#[test]
fn a_list_gives_every_number_slot_its_longest_numbers_key() {
    assert_eq!(numbers("8. a\n9. b\n10. c"), vec![vec![key(2, b'.')]; 3]);
    assert_eq!(
        numbers("10. a\n    - b"),
        [vec![key(2, b'.')], vec![key(2, b'.')]]
    );
    assert_eq!(numbers("- a\n  1) b"), [vec![], vec![key(1, b')')]]);
    assert_eq!(
        numbers("- a\n  1) b\n- c"),
        [vec![], vec![key(1, b')')], vec![]]
    );
    assert_eq!(numbers("- a\n- b"), [vec![], vec![]]);
}

#[test]
fn no_gutter_items_bullet_has_a_dot() {
    assert_eq!(concealed("- a\n- b"), [("-", "-", ""), ("-", "-", "")]);
    assert_eq!(concealed("1. a"), [("1.", "1.", "")]);
    assert_eq!(concealed("- > - a"), [("-", "-", ""), ("-", "-", "•")]);
    assert_eq!(concealed("> - a"), [(">", ">", ""), ("-", "-", "")]);
}

/// The item conceal runs through the last mark: a caret at 0 or at the mark's end touches it, one
/// at the text start does not (D2).
#[test]
fn the_item_conceal_runs_through_the_last_mark() {
    assert_eq!(concealed("- [ ] a"), [("- [ ]", "- [ ]", "")]);
    assert_eq!(concealed("- 1. a"), [("- 1.", "- 1.", "")]);
    let caret = |at| ByteOffset::new(at)..ByteOffset::new(at);
    for (text, mark_end, text_start) in [("- a", 1, 2), ("- [ ] a", 5, 6), ("1. a", 2, 3)] {
        let markdown = parse(text);
        let [conceal] = markdown.conceals.as_slice() else {
            panic!("{text:?}: {:?}", markdown.conceals);
        };
        assert!(!is_active(conceal, &caret(0)), "{text:?} at 0");
        assert!(
            !is_active(conceal, &caret(mark_end)),
            "{text:?} at its mark's end"
        );
        assert!(
            is_active(conceal, &caret(text_start)),
            "{text:?} at its text"
        );
    }
}

/// An empty item part emits no conceal, and one of blanks only yields no mark (D1, D2).
#[test]
fn an_item_part_with_no_mark_has_no_mark_and_an_empty_one_no_conceal() {
    let start = ByteOffset::new(4);
    let item = ItemPart {
        start,
        indent: 1,
        numbers: 0..0,
        task: None,
    };
    assert_eq!(item.marks("  ").count(), 0);
    let gutter = Gutter {
        range: ByteOffset::new(4)..start,
        quotes: Vec::new(),
        depth: 0,
        item: Some(item),
    };
    assert_eq!(gutter_conceals("- a\nb", &[gutter]).count(), 0);
}

/// `gutter` counts columns before each `>`, as pulldown-cmark nests them (D1, M1).
#[test]
fn a_tab_after_a_quotes_blank_still_leads_the_next_quote() {
    let gutters = parse("> \t> a").gutters;
    let quotes: Vec<usize> = gutters[0].quotes.iter().map(|quote| quote.get()).collect();
    assert_eq!((quotes, gutters[0].depth), (vec![0, 3], 2));
    let text = "> \t> - [ ] a";
    let gutter = &parse(text).gutters[0];
    assert_eq!(gutter.quotes.len(), 2);
    assert_eq!(part(text, gutter), Some(expect("- [ ] ", 0, &[], true)));
}

/// An item's other lines hide their blanks below its hang column, so they start at its text (D11).
#[test]
fn an_items_other_lines_hide_their_blanks_below_its_hang_column() {
    for (text, line, source, indent) in [
        ("- a\n  b", 1, "  ", 1),
        ("- a\nb", 1, "", 1),
        ("- a\n  - b\n    c", 2, "    ", 2),
        ("- a\n  - b\n  c", 2, "  ", 2),
        ("-\ta\n\tb", 1, "\t", 1),
        ("-\ta\n    b", 1, "    ", 1),
        ("-   a\n    b", 1, "  ", 1),
        ("> - a\n>   b", 1, "  ", 1),
        ("> - a\nb", 1, "", 1),
        ("1. [ ] a\n   b", 1, "   ", 2),
        ("- a\n  > - b", 1, "  ", 1),
    ] {
        assert_eq!(
            line_part(text, line),
            Some(expect(source, indent, &[], false)),
            "{text:?}"
        );
    }
    assert_eq!(numbers("10. a\n    b"), vec![vec![key(2, b'.')]; 2]);
}

#[test]
fn an_items_blanks_conceal_whole_and_an_empty_part_not_at_all() {
    assert_eq!(concealed("- a\n  b"), [("-", "-", ""), ("  ", "  ", "")]);
    assert_eq!(concealed("- a\nb"), [("-", "-", "")]);
}

/// A fenced block's lines all take the item part, its blank line too; a blank line elsewhere
/// takes no gutter (D11).
#[test]
fn a_code_blocks_lines_take_the_item_part_and_other_blank_lines_none() {
    let text = "- a\n\n  ```\n  x\n  \n  ```\n\n  b";
    for line in 2..=5 {
        assert_eq!(
            line_part(text, line),
            Some(expect("  ", 1, &[], false)),
            "line {line}"
        );
    }
    let starts = |text: &str| -> Vec<usize> {
        let gutters = parse(text).gutters.into_iter();
        gutters.map(|gutter| gutter.range.start.get()).collect()
    };
    assert_eq!(starts(text), [0, 5, 11, 15, 18, 25]);
    // An open fence ends at its last blank line's start, which it still holds.
    assert_eq!(starts("- a\n\n  ```\n  x\n\n"), [0, 5, 11, 15]);
    assert_eq!(starts("- a\n\n  b"), [0, 5]);
    assert_eq!(starts("> - a\n>\n>   b").len(), 3);
    assert_eq!(line_part("> - a\n>\n>   b", 1), None);
}

/// A code block's line hides its blanks up to its fence or its code text, where pulldown-cmark
/// starts it past the container's and the fence's indent, however far the hang column lies (D11).
#[test]
fn a_code_line_in_an_item_hides_its_blanks_up_to_its_code_text() {
    for (text, line, source, indent) in [
        (
            "- [ ] a\n\n  ```\n  def f():\n      return 1\n  ```",
            3,
            "  ",
            1,
        ),
        (
            "- [ ] a\n\n  ```\n  def f():\n      return 1\n  ```",
            4,
            "  ",
            1,
        ),
        ("1. [ ] a\n   ```\n     x\n   ```", 2, "   ", 2),
        ("- - a\n\n  b\n\n  ```\n    x\n  ```", 5, "  ", 1),
        ("- a\n  ```\n\tx\n  ```", 2, "", 1),
        ("- a\n\n     ```\n     x\n       y\n     ```", 2, "     ", 1),
        ("- a\n\n     ```\n     x\n       y\n     ```", 3, "     ", 1),
        ("- a\n\n     ```\n     x\n       y\n     ```", 4, "     ", 1),
        ("> - a\n>   ```\n>     x\n>   ```", 2, "  ", 1),
        ("- a\n  > -     x", 1, "  ", 1),
    ] {
        assert_eq!(
            line_part(text, line),
            Some(expect(source, indent, &[], false)),
            "{text:?} line {line}"
        );
    }
}
