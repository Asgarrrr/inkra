//! An open element as the parse's one pass tracks it: its styles, its end, and what its markers
//! conceal.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Tag};

use super::without_line_ending;
use crate::Styles;

/// An open element: the union of its and its ancestors' tags, its end, and its range when its
/// markers can hide.
#[derive(Clone)]
pub(super) struct Open {
    pub(super) styles: Styles,
    /// What a line-start prefix in this element carries: the parent's styles in a code block or a
    /// code span, so its indents match the opening line's.
    pub(super) indent: Styles,
    pub(super) end: usize,
    pub(super) element: Option<Range<usize>>,
    /// A quote whose ancestors are all quotes.
    pub(super) leading: bool,
}

impl Open {
    /// The element `tag` starts inside `parent`, the innermost open one.
    pub(super) fn enter(text: &str, tag: &Tag<'_>, range: &Range<usize>, parent: Option<&Open>) -> Self {
        let mut inherited = parent.map_or(Styles::NONE, |open| open.styles);
        // A checked item's own text is struck; a block nested in it is not (task boxes spec, D7).
        if matches!(tag, Tag::List(_) | Tag::BlockQuote(_) | Tag::CodeBlock(_)) {
            inherited = inherited.without(Styles::DONE);
        }
        let styles = inherited | tag_styles(tag);
        Self {
            styles,
            indent: if matches!(tag, Tag::CodeBlock(_)) {
                inherited
            } else {
                styles
            },
            end: range.end,
            element: concealable(text, tag, range),
            // A quote leads its line's prefix when every ancestor is a quote; a list item breaks it.
            leading: matches!(tag, Tag::BlockQuote(_)) && parent.is_none_or(|open| open.leading),
        }
    }

    /// A code span, backticks included: no tag opens it, and its backticks always hide.
    pub(super) fn code_span(range: &Range<usize>, parent: Option<&Open>) -> Self {
        let inherited = parent.map_or(Styles::NONE, |open| open.styles);
        Self {
            styles: inherited | Styles::CODE,
            indent: inherited,
            end: range.end,
            element: Some(range.clone()),
            leading: false,
        }
    }
}

fn tag_styles(tag: &Tag<'_>) -> Styles {
    match tag {
        Tag::Heading { level, .. } => Styles::heading(*level),
        Tag::Strong => Styles::STRONG,
        Tag::Emphasis => Styles::EMPHASIS,
        Tag::Strikethrough => Styles::STRIKETHROUGH,
        Tag::CodeBlock(_) => Styles::CODE | Styles::CODE_BLOCK,
        Tag::Link { .. } | Tag::Image { .. } => Styles::LINK,
        Tag::BlockQuote(_) => Styles::QUOTE,
        Tag::MetadataBlock(_) => Styles::FRONTMATTER,
        _ => Styles::NONE,
    }
}

/// The range whose touch reveals this element's markers, or `None` when they never hide (spec D2).
/// A fenced block's fences hide too (code-blocks spec, D2).
fn concealable(text: &str, tag: &Tag<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    matches!(
        tag,
        Tag::Strong
            | Tag::Emphasis
            | Tag::Strikethrough
            | Tag::Link { .. }
            | Tag::CodeBlock(CodeBlockKind::Fenced(_))
    )
    .then(|| without_line_ending(text, range))
}
