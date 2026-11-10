use std::ops::Range;

use inkra_text::ByteOffset;

use crate::{LinkContext, link_context};

fn context(text: &str, link: Range<usize>) -> Option<LinkContext> {
    link_context(text, ByteOffset::new(link.start)..ByteOffset::new(link.end))
}

#[test]
fn the_window_holds_the_link_and_its_line() {
    let cases = [
        ("ab [[x]] cd", 3..8, "ab [[x]] cd", 3..8),
        ("one\ntwo [[x]]\nthree", 8..13, "two [[x]]", 4..9),
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
fn a_range_off_the_text_or_its_chars_has_no_context() {
    let reversed = Range { start: 3, end: 2 };
    for (text, link) in [("[[x]]", 0..6), ("[[x]]", reversed), ("é[[x]]", 1..7)] {
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
