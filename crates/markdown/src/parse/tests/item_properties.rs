//! Item parts checked against a pulldown-cmark oracle of their own (task boxes spec, D1, D10,
//! D11): every marker line is well formed, and exactly the held lines hold an item part.

use inkra_text::ByteOffset;
use proptest::prelude::*;
use pulldown_cmark::{Event, Parser, Tag};

use super::super::parse;
use super::syntax_fragment;
use crate::cmark::options;
use crate::{Gutter, ItemMarker, Markdown, Styles};

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
/// and no other line holds one; one where no gutter item opens holds blanks only (D11).
fn held_lines_hold_an_item_part(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    let items = gutter_items(text);
    let marker_lines: Vec<usize> = items
        .iter()
        .map(|(marker, _, _, _)| text[..*marker].rfind('\n').map_or(0, |newline| newline + 1))
        .collect();
    let mut held = Vec::new();
    for &(marker, _, _, end) in &items {
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
        let part = &text[gutter.quote_end().get()..gutter.range.end.get()];
        prop_assert!(
            marker_lines.contains(&line) || part.bytes().all(|byte| b" \t".contains(&byte)),
            "item part {:?} at {} in {:?}",
            part,
            line,
            text
        );
    }
    Ok(())
}

/// No item part covers a byte of a code block's text or fences, or of an HTML block's (D11).
fn no_item_part_covers_leaf_text(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    let terminated = format!("{text}\n");
    let html = Parser::new_ext(&terminated, options())
        .into_offset_iter()
        .filter(|(event, _)| matches!(event, Event::Html(_)))
        .map(|(_, range)| range);
    let leaves: Vec<_> = markdown
        .spans
        .iter()
        .filter(|span| span.styles.contains(Styles::CODE_BLOCK))
        .map(|span| span.range.start.get()..span.range.end.get())
        .chain(html)
        .collect();
    for gutter in &markdown.gutters {
        let Some(item) = &gutter.item else { continue };
        let part = item.start.get()..gutter.range.end.get();
        prop_assert!(
            leaves
                .iter()
                .all(|leaf| leaf.end <= part.start || part.end <= leaf.start),
            "item part {:?} covers a leaf's text in {:?}",
            part,
            text
        );
    }
    Ok(())
}

/// Items holding code and HTML blocks: markers, quotes, blanks and tabs that indent a fence, a
/// code line or an HTML line. Each property here also checks that `parse` never panics on them.
fn item_code_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("- ".to_string()),
        Just("- [ ] ".to_string()),
        Just("1. [ ] ".to_string()),
        Just("2) ".to_string()),
        Just("> ".to_string()),
        Just(">".to_string()),
        Just(" ".to_string()),
        Just("  ".to_string()),
        Just("\t".to_string()),
        Just("```\n".to_string()),
        Just("```".to_string()),
        Just("~~~".to_string()),
        Just("<div>".to_string()),
        Just("x".to_string()),
        Just("\n".to_string()),
    ]
}

proptest! {
    #[test]
    fn no_item_part_covers_leaf_text_in_items_holding_code(
        fragments in prop::collection::vec(item_code_fragment(), 0..40)
    ) {
        no_item_part_covers_leaf_text(&fragments.concat())?;
    }

    #[test]
    fn held_lines_hold_an_item_part_in_items_holding_code(
        fragments in prop::collection::vec(item_code_fragment(), 0..40)
    ) {
        held_lines_hold_an_item_part(&fragments.concat())?;
    }

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
