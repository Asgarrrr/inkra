use std::cell::Cell;

use proptest::prelude::*;
use pulldown_cmark::{Event, Parser, Tag};

use super::front_matter;
use crate::cmark::options;

/// `front_matter` of `text`, checked against pulldown-cmark.
fn lines(text: &str) -> Option<usize> {
    let found = front_matter(text.split('\n'));
    assert_eq!(found, oracle(text), "{text:?}");
    found
}

#[test]
fn a_yaml_block_on_the_first_line_is_front_matter() {
    assert_eq!(lines("---\ntitle: Plan\n---\nBody."), Some(3));
    assert_eq!(lines("---\na: b\nc: d\n---"), Some(4));
    assert_eq!(
        lines("---\ntitle: Plan\n...\nBody."),
        Some(3),
        "`...` closes"
    );
    assert_eq!(
        lines("---  \na: b\n---  \nBody."),
        Some(3),
        "trailing spaces"
    );
    assert_eq!(
        lines("---\na: b\n\nc: d\n---"),
        Some(5),
        "a later blank line"
    );
}

#[test]
fn anything_else_is_not() {
    assert_eq!(lines("---\ntitle: Plan\n"), None, "unclosed");
    assert_eq!(lines("---\n---\nBody."), None, "closed at once");
    assert_eq!(lines("---\n\na: b\n---"), None, "blank first line");
    assert_eq!(lines("---"), None);
    assert_eq!(lines("----\na: b\n----"), None, "four dashes");
    assert_eq!(lines("--- x\na: b\n---"), None, "text after the fence");
    assert_eq!(
        lines("---\x0b\na: b\n---"),
        None,
        "a vertical tab after the fence"
    );
    assert_eq!(lines(" ---\na: b\n---"), None, "indented");
    assert_eq!(lines("\n---\na: b\n---"), None, "not on the first line");
    assert_eq!(
        lines("---\na: b\n ---\n----\n---\t"),
        None,
        "no closing fence"
    );
    assert_eq!(lines("+++\na: b\n+++"), None, "pluses are off");
}

#[test]
fn no_line_is_read_past_the_closing_fence() {
    let read = Cell::new(0);
    let text = ["---", "a: b", "---", "Body.", "More."];
    let found = front_matter(text.iter().inspect(|_| read.set(read.get() + 1)).copied());
    assert_eq!((found, read.get()), (Some(3), 3));
}

fn oracle(text: &str) -> Option<usize> {
    Parser::new_ext(text, options())
        .into_offset_iter()
        .find_map(|(event, range)| match event {
            Event::Start(Tag::MetadataBlock(_)) if range.start == 0 => {
                Some(text[..range.end].matches('\n').count() + 1)
            }
            _ => None,
        })
}

fn line() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "---", "--- ", "---\t", "---\r", "---\x0b", "----", "...", "...\r", "... ", "....", ".. .",
        "--- x", " ---", "---.", "a: b", "a\r", "", "  ", "\t", "\x0b", "\x0c", "# h", "x", "- y",
    ])
}

proptest! {
    #[test]
    fn it_agrees_with_parse(lines in prop::collection::vec(line(), 0..8), newline: bool) {
        let mut text = lines.join("\n");
        if newline {
            text.push('\n');
        }
        prop_assert_eq!(front_matter(text.split('\n')), oracle(&text), "{:?}", text);
    }
}
