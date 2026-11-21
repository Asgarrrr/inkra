use inkra_text::ByteOffset;

use crate::parse;

/// Each item's marker.
fn items(text: &str) -> Vec<usize> {
    let items = parse(text).structure.items.into_iter();
    items.map(ByteOffset::get).collect()
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
