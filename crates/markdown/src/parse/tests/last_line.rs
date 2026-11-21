//! The last line reads as if a newline ended it, and no output reaches past the text.

use proptest::prelude::*;

use super::syntax_fragment;
use crate::{Markdown, parse};

/// Every offset a parse holds.
fn offsets(markdown: &Markdown) -> Vec<usize> {
    let mut all = Vec::new();
    let mut range = |range: &std::ops::Range<inkra_text::ByteOffset>| {
        all.extend([range.start.get(), range.end.get()]);
    };
    markdown.spans.iter().for_each(|span| range(&span.range));
    for conceal in &markdown.conceals {
        range(&conceal.element);
        range(&conceal.marker);
    }
    for heading in &markdown.headings {
        range(&heading.range);
        heading.marker.iter().for_each(&mut range);
    }
    for block in &markdown.code_blocks {
        range(&block.range);
        block.language.iter().for_each(&mut range);
    }
    for gutter in &markdown.gutters {
        range(&gutter.range);
        gutter
            .quotes
            .iter()
            .for_each(|&quote| range(&(quote..quote + 1)));
        if let Some(item) = &gutter.item {
            range(&(item.start..item.start));
            item.task.iter().for_each(|task| range(&task.checkbox));
        }
    }
    let structure = &markdown.structure;
    let openers = structure.items.iter().chain(&structure.quotes);
    openers.for_each(|&opener| range(&(opener..opener)));
    structure.skipped.iter().for_each(&mut range);
    all
}

/// A construct whose meaning a final newline could change.
fn ending() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "- [ ]",
        "- [x]",
        "1. [X]",
        "> - [ ]",
        "```",
        "~~~ rs\na",
        "a\n---",
        "a\n===",
        "| a |\n|---|",
        "a\\",
        "a  ",
        "<div>",
        "<!--",
        "[a]: b",
        "# a #",
        "- ",
        "-",
        "**a",
        "`a",
    ])
    .prop_map(str::to_owned)
}

proptest! {
    #[test]
    fn the_last_line_parses_as_if_a_newline_ended_it(
        fragments in prop::collection::vec(syntax_fragment(), 0..20),
        ending in ending(),
    ) {
        let text = fragments.concat() + &ending;
        let markdown = parse(&text);
        prop_assert_eq!(&markdown, &parse(&format!("{text}\n")));
        let past = offsets(&markdown).into_iter().find(|&offset| offset > text.len());
        prop_assert_eq!(past, None, "{:?}", text);
    }

    #[test]
    fn no_offset_reaches_past_any_text(text in any::<String>()) {
        let past = offsets(&parse(&text)).into_iter().find(|&offset| offset > text.len());
        prop_assert_eq!(past, None, "{:?}", text);
    }
}
