use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, LinkType, Parser, Tag};

use crate::parse::options;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The whole link, `!` included for an embed.
    pub range: Range<ByteOffset>,
    pub kind: LinkKind,
    pub embed: bool,
    /// As written, trimmed: fragment kept, not percent-decoded, never empty.
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `[[target]]` or `![[target]]`.
    Wiki,
    /// Every other form: inline, reference, autolink.
    Inline,
}

/// Every link in `text`, in document order, with the parser and options `parse` uses, so a
/// link inside code, raw HTML or frontmatter is not one.
#[must_use]
pub fn links(text: &str) -> Vec<Link> {
    let mut found = Vec::new();
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        let (link_type, dest_url, embed) = match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => (link_type, dest_url, false),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => (link_type, dest_url, true),
            _ => continue,
        };
        let Some(target) = target(link_type, &dest_url) else {
            continue;
        };
        let kind = if matches!(link_type, LinkType::WikiLink { .. }) {
            LinkKind::Wiki
        } else {
            LinkKind::Inline
        };
        found.push(Link {
            range: ByteOffset::new(range.start)..ByteOffset::new(range.end),
            kind,
            embed,
            target,
        });
    }
    found.sort_by_key(|link| link.range.start);
    found
}

/// The innermost link containing `offset`, else the innermost one ending at it, so a caret
/// just past a link still follows it and one between two links follows the second.
#[must_use]
pub fn link_at(text: &str, offset: ByteOffset) -> Option<Link> {
    links(text)
        .into_iter()
        .filter(|link| link.range.start <= offset && offset <= link.range.end)
        .min_by_key(|link| (link.range.end == offset, link.range.end - link.range.start))
}

fn target(link_type: LinkType, dest_url: &str) -> Option<String> {
    let dest = dest_url.trim();
    // `\|` escapes a wikilink's pipe, and pulldown-cmark leaves the backslash on the target.
    let dest = match link_type {
        LinkType::WikiLink { .. } => dest.strip_suffix('\\').unwrap_or(dest).trim_end(),
        _ => dest,
    };
    if dest.is_empty() {
        return None;
    }
    Some(match link_type {
        LinkType::Email => format!("mailto:{dest}"),
        _ => dest.to_owned(),
    })
}

#[cfg(test)]
mod tests;
