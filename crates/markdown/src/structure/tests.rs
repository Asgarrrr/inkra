use inkra_text::ByteOffset;

use crate::parse;

/// Each item's marker.
fn items(text: &str) -> Vec<usize> {
    let items = parse(text).structure.items.into_iter();
    items.map(|item| item.marker.get()).collect()
}

/// Each item's marker, end, parent and list.
fn tree(text: &str) -> Vec<(usize, usize, Option<u32>, u32)> {
    let items = parse(text).structure.items.into_iter();
    items
        .map(|item| (item.marker.get(), item.end.get(), item.parent, item.list))
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

/// The marker past a tab's leftover columns and past the quote markers before it on its line.
#[test]
fn an_item_holds_its_marker() {
    for (text, want) in [
        ("- a", vec![0]),
        ("- 1. a", vec![0, 2]),
        ("-", vec![0]),
        ("  -\t ", vec![2]),
        ("12) a", vec![0]),
        ("- > - a", vec![0, 4]),
        ("-\t- a", vec![0, 2]),
        (">\t- a", vec![2]),
        ("- a\n\n  -\t  ", vec![0, 7]),
        ("a\n-", vec![]),
    ] {
        assert_eq!(items(text), want, "{text:?}");
    }
}

/// A child's parent is the item it nests in; an item ends before its last line ending, so the
/// last line's added `\n` reaches no offset.
#[test]
fn an_item_knows_its_end_its_parent_and_its_list() {
    assert_eq!(
        tree("- a\n  - b\n- c"),
        vec![(0, 9, None, 0), (6, 9, Some(0), 1), (10, 13, None, 0)]
    );
    assert_eq!(
        tree("- a\nb\n\n  1. c\n"),
        vec![(0, 13, None, 0), (9, 13, Some(0), 1)]
    );
}

/// Siblings share a list, not merely a parent: a block between two items, or a quote ending,
/// starts a new list.
#[test]
fn a_block_between_items_starts_a_new_list() {
    let lists = |text| {
        tree(text)
            .into_iter()
            .map(|item| item.3)
            .collect::<Vec<_>>()
    };
    assert_eq!(lists("1. a\n\npara\n\n2. x"), vec![0, 1]);
    assert_eq!(lists("> - a\n\n- b"), vec![0, 1]);
    assert_eq!(lists("- a\n* b\n- c"), vec![0, 1, 2]);
    assert_eq!(lists("- a\n  - b\n- c\n  - d"), vec![0, 1, 0, 2]);
}

/// A tab's leftover columns start the range on the line before; the item still nests by its
/// marker.
#[test]
fn an_item_starting_its_range_early_still_nests_by_its_marker() {
    let parents = tree("- a\n\n  -\t  b").into_iter().map(|item| item.2);
    assert_eq!(parents.collect::<Vec<_>>(), vec![None, Some(0)]);
}

#[test]
fn a_quote_and_an_item_open_in_document_order() {
    assert_eq!((quotes("> - a"), items("> - a")), (vec![0], vec![2]));
    assert_eq!((quotes("- > a"), items("- > a")), (vec![2], vec![0]));
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
