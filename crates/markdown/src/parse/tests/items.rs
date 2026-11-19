//! Item parts (task boxes spec, D1, D2, D11): a gutter item's marker line shows its markers in
//! slots, nested by level, and its numbers take their list's width key.

use inkra_text::ByteOffset;
use proptest::prelude::*;
use pulldown_cmark::{Event, Parser, Tag};

use super::super::parse;
use super::{concealed, syntax_fragment};
use crate::cmark::options;
use crate::gutters::gutter_conceals;
use crate::{Gutter, ItemMarker, ItemPart, Markdown, WidthKey, is_active};

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

/// A gutter item as the oracle reads it: its marker, the mark its line should show for it, whether
/// it is a task, and its range's end.
type Oracle = (usize, Option<ItemMarker>, bool, usize);

/// Each gutter item, in order: a task's own bullet gives way to its box, a number shows as typed.
/// Read by a pulldown-cmark pass of its own.
fn gutter_items(text: &str) -> Vec<Oracle> {
    let text = format!("{text}\n");
    // Each open element: whether it is in the gutter, and whether it is a leading quote.
    let mut open: Vec<(bool, bool)> = Vec::new();
    let mut items = Vec::new();
    let mut pending: Option<usize> = None;
    for (event, range) in Parser::new_ext(&text, options()).into_offset_iter() {
        if let Some(index) = pending.take() {
            match event {
                Event::Start(Tag::Paragraph) => pending = Some(index),
                Event::TaskListMarker(_) => items[index] = task(&items[index]),
                _ => {}
            }
        }
        match event {
            Event::Start(tag) => {
                let parent = open.last().copied();
                let leading = matches!(tag, Tag::BlockQuote(_)) && parent.is_none_or(|p| p.1);
                let listed = matches!(tag, Tag::List(_) | Tag::Item) && parent.is_none_or(|p| p.0);
                if matches!(tag, Tag::Item) && listed {
                    let bytes = &text.as_bytes()[range.start..];
                    let skip = bytes.iter().take_while(|b| b" \t\n>".contains(b)).count();
                    let marker = range.start + skip;
                    let digits = bytes[skip..]
                        .iter()
                        .take_while(|b| b.is_ascii_digit())
                        .count();
                    let mark = if digits == 0 {
                        ItemMarker::Bullet
                    } else {
                        ItemMarker::Number(
                            ByteOffset::new(marker)..ByteOffset::new(marker + digits + 1),
                        )
                    };
                    pending = Some(items.len());
                    items.push((marker, Some(mark), false, range.end));
                }
                open.push((leading || listed, leading));
            }
            Event::End(_) => {
                open.pop();
            }
            _ => {}
        }
    }
    items
}

fn task(item: &Oracle) -> Oracle {
    let mark = item.1.clone().filter(|mark| *mark != ItemMarker::Bullet);
    (item.0, mark, true, item.3)
}

/// Every gutter item's marker line holds a gutter whose marks are its markers, its box its task's,
/// with all its `>` and none between them and its markers (D1, D10, D11).
fn marker_lines_are_well_formed(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    for (marker, _, _, _) in gutter_items(text) {
        let line = text[..marker].rfind('\n').map_or(0, |newline| newline + 1);
        let gutter = gutter_on(&markdown, line);
        let gutter = gutter
            .ok_or_else(|| TestCaseError::fail(format!("no gutter at {line} in {text:?}")))?;
        let item = gutter
            .item
            .as_ref()
            .expect("a marker line has an item part");
        prop_assert_eq!(gutter.quotes.len(), gutter.depth, "{:?}", text);
        prop_assert!(
            !text[gutter.quote_end().get()..marker].contains('>'),
            "{:?}",
            text
        );
        let expected: Vec<_> = gutter_items(text)
            .into_iter()
            .filter(|(at, _, _, _)| text[..*at].rfind('\n').map_or(0, |n| n + 1) == line)
            .collect();
        let marks: Vec<_> = item
            .marks(&text[item.start.get()..gutter.range.end.get()])
            .collect();
        let wanted: Vec<_> = expected
            .iter()
            .filter_map(|(_, mark, _, _)| mark.clone())
            .collect();
        prop_assert_eq!(marks, wanted, "{:?}", text);
        let boxed = expected.last().is_some_and(|(_, _, task, _)| *task);
        prop_assert_eq!(item.task.is_some(), boxed, "{:?}", text);
    }
    Ok(())
}

fn gutter_on(markdown: &Markdown, line: usize) -> Option<&Gutter> {
    markdown
        .gutters
        .iter()
        .find(|gutter| gutter.range.start.get() == line)
}

/// Every line a gutter item holds that is not blank, of blanks and `>` only, holds an item part,
/// and no other line holds one (D11).
fn held_lines_hold_an_item_part(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    let mut held = Vec::new();
    for (marker, _, _, end) in gutter_items(text) {
        let mut line = text[..marker].rfind('\n').map_or(0, |newline| newline + 1);
        while line < end.min(text.len() + 1) {
            let line_end = text[line..].find('\n').map_or(text.len(), |len| line + len);
            if !text[line..line_end]
                .bytes()
                .all(|byte| b" \t>".contains(&byte))
            {
                let item = gutter_on(&markdown, line).and_then(|gutter| gutter.item.as_ref());
                prop_assert!(item.is_some(), "no item part at {} in {:?}", line, text);
            }
            held.push(line);
            line = line_end + 1;
        }
    }
    for gutter in markdown
        .gutters
        .iter()
        .filter(|gutter| gutter.item.is_some())
    {
        let line = gutter.range.start.get();
        prop_assert!(
            held.contains(&line),
            "an item part at {} in {:?}",
            line,
            text
        );
    }
    Ok(())
}

proptest! {
    #[test]
    fn held_lines_hold_an_item_part_on_block_constructs(
        fragments in prop::collection::vec(syntax_fragment(), 0..40)
    ) {
        held_lines_hold_an_item_part(&fragments.concat())?;
    }

    #[test]
    fn held_lines_hold_an_item_part_on_list_text(text in "[ \t>*+19.)\\[\\]xa\n-]{0,40}") {
        held_lines_hold_an_item_part(&text)?;
    }

    #[test]
    fn marker_lines_are_well_formed_on_block_constructs(
        fragments in prop::collection::vec(syntax_fragment(), 0..40)
    ) {
        marker_lines_are_well_formed(&fragments.concat())?;
    }

    #[test]
    fn marker_lines_are_well_formed_on_list_text(text in "[ \t>*+19.)\\[\\]xa\n-]{0,40}") {
        marker_lines_are_well_formed(&text)?;
    }
}
