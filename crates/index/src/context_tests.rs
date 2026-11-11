use std::ops::Range;

use inkra_text::ByteOffset;
use proptest::prelude::*;

use crate::{LinkContext, Source, link_context};

fn context(text: &str, link: Range<usize>) -> Option<LinkContext> {
    link_context(text, ByteOffset::new(link.start)..ByteOffset::new(link.end))
}

#[test]
fn the_window_holds_the_link_and_its_line() {
    let cases = [
        ("ab [[x]] cd", 3..8, "ab [[x]] cd", 3..8),
        ("one\ntwo [[x]]\nthree", 8..13, "two [[x]]", 4..9),
        ("x ![[p.png]]", 2..12, "x ![[p.png]]", 2..12),
    ];
    for (text, link, window, inside) in cases {
        let found = LinkContext {
            text: window.into(),
            link: inside,
        };
        assert_eq!(context(text, link), Some(found), "{text:?}");
    }
}

#[test]
fn a_range_off_the_text_its_chars_or_a_bracket_has_no_context() {
    let reversed = Range { start: 3, end: 2 };
    let cases = [
        ("[[x]]", 0..6),
        ("[[x]]", reversed),
        ("é[[x]]", 1..7),
        ("ab [[x]]", 1..8),
    ];
    for (text, link) in cases {
        assert_eq!(context(text, link.clone()), None, "{text:?} at {link:?}");
    }
}

#[test]
fn a_long_link_is_clamped_to_the_window() {
    let text = format!("{}[[{}]]", "b".repeat(80), "a".repeat(10_000));
    let Some(found) = context(&text, 80..text.len()) else {
        panic!("the link is in the text");
    };
    assert_eq!((found.text.len(), found.link), (160, 60..160));
}

/// The two inputs proptest once shrank to; kept as plain tests so no file lookup decides whether they run.
#[test]
fn a_wikilink_opened_inside_another_proves_on_its_text() {
    for text in ["[[![[|]](a)]]", "[[![[]](|)]]"] {
        for link in Source::parse(text).links.iter() {
            assert!(
                link_context(text, link.range.clone()).is_some(),
                "{text}: {link:?}"
            );
        }
    }
}

proptest! {
    #[test]
    fn every_parsed_link_proves_on_its_text(
        text in "(\\[\\[|\\]\\]|!\\[\\[|\\[|\\]|\\(|\\)|!|\\||#|a|é|\\.md| |\\n|`){0,40}"
    ) {
        for link in Source::parse(&text).links.iter() {
            prop_assert!(link_context(&text, link.range.clone()).is_some(), "{link:?}");
        }
    }
}
