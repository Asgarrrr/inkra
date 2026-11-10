//! The part of a source's line the backlinks panel shows around a link.

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::window;

/// A link's window of its line, and the link inside it, clamped to the window's end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkContext {
    pub text: Box<str>,
    pub link: Range<usize>,
}

/// `None` unless `link` is in `text` and on char boundaries: a matching stamp does not prove it.
#[must_use]
pub fn link_context(text: &str, link: Range<ByteOffset>) -> Option<LinkContext> {
    let (start, end) = (link.start.get(), link.end.get());
    // `get` checks the order, the bounds and both char boundaries.
    text.get(start..end)?;
    let window = window::around(text, start..start);
    Some(LinkContext {
        text: text[window.clone()].into(),
        link: start - window.start..end.min(window.end) - window.start,
    })
}
