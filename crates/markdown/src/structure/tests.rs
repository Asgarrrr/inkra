use std::ops::Range;

use inkra_text::ByteOffset;

use crate::parse;

/// Each item's marker and content start.
fn items(text: &str) -> Vec<(Range<usize>, usize)> {
    let structure = parse(text).structure;
    let items = structure.items.into_iter();
    items
        .map(|item| {
            let marker = item.marker.start.get()..item.marker.end.get();
            (marker, item.content.get())
        })
        .collect()
}

fn quotes(text: &str) -> Vec<usize> {
    let quotes = parse(text).structure.quotes.into_iter();
    quotes.map(ByteOffset::get).collect()
}

fn skipped(text: &str) -> Vec<&str> {
    let skipped = parse(text).structure.skipped.into_iter();
    skipped
        .map(|range| &text[range.start.get()..range.end.get()])
        .collect()
}

#[test]
fn an_item_holds_its_marker_and_its_content_start() {
    for (text, want) in [
        ("- a", vec![(0..1, 2)]),
        ("- 1. a", vec![(0..1, 2), (2..4, 5)]),
        ("-", vec![(0..1, 1)]),
        ("  -\t ", vec![(2..3, 5)]),
        ("12) a", vec![(0..3, 4)]),
        ("-   a", vec![(0..1, 4)]),
        ("-     a", vec![(0..1, 2)]),
        ("-\t  ", vec![(0..1, 2)]),
        ("a\n-", vec![]),
    ] {
        assert_eq!(items(text), want, "{text:?}");
    }
}

/// Columns count from the line's start, through the openers before the marker.
#[test]
fn an_items_blanks_count_in_columns_from_the_lines_start() {
    for (text, want) in [
        ("- > - a", vec![(0..1, 2), (4..5, 6)]),
        ("- >\t- a", vec![(0..1, 2), (4..5, 6)]),
        (">\t- a", vec![(2..3, 4)]),
        ("- a\n\n  -\t  ", vec![(0..1, 2), (7..8, 11)]),
        ("- a\n\n -\t  ", vec![(0..1, 2), (6..7, 10)]),
    ] {
        assert_eq!(items(text), want, "{text:?}");
    }
}

#[test]
fn a_quote_and_an_item_open_in_document_order() {
    assert_eq!(
        (quotes("> - a"), items("> - a")),
        (vec![0], vec![(2..3, 4)])
    );
    assert_eq!(
        (quotes("- > a"), items("- > a")),
        (vec![2], vec![(0..1, 2)])
    );
}

#[test]
fn a_block_is_skipped_from_its_first_lines_start_without_its_last_newline() {
    for (text, want) in [
        ("```\na\n```\n", vec!["```\na\n```"]),
        ("- a\n\n  ```\n  b\n  ```\n", vec!["  ```\n  b\n  ```"]),
        ("---\nt: x\n---\nb", vec!["---\nt: x\n---"]),
        ("| a |\n|---|\n| b |\n", vec!["| a |\n|---|\n| b |"]),
        ("<div>\nb\n</div>\n", vec!["<div>\nb\n</div>"]),
        ("# a\nb", vec!["# a"]),
        ("- # a", vec!["- # a"]),
        ("> # a", vec!["> # a"]),
        ("- > - [ ] a\nb", vec!["- > - [ ] a"]),
        (
            "a\n```\nb\n```\n- > - [ ] c\n# d",
            vec!["```\nb\n```", "- > - [ ] c", "# d"],
        ),
    ] {
        assert_eq!(skipped(text), want, "{text:?}");
    }
}

#[test]
fn a_setext_heading_and_a_boxed_task_are_not_skipped() {
    for text in ["a\n===", "a\n- ", "#a\n===", "- [ ] a", "- [ ] # a"] {
        assert_eq!(skipped(text), Vec::<&str>::new(), "{text:?}");
    }
}
