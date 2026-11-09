use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use foldhash::fast::RandomState;
use inkra_markdown::links;
use inkra_text::ByteOffset;

use crate::key::Key;
use crate::target::{NoteRef, Style, Target};

/// Bytes of a link's line kept for the backlinks panel, and how many of them precede the link.
const CONTEXT: usize = 160;
const BEFORE: usize = 60;

/// A note's links to other notes, parsed from its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    pub links: Arc<[SourceLink]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLink {
    /// As written, `.md` stripped, `/`-separated.
    pub path: Arc<str>,
    pub fragment: Option<Box<str>>,
    pub md: bool,
    pub style: Style,
    pub range: Range<ByteOffset>,
    /// Part of the link's line, for display only: nothing resolves by it.
    pub context: String,
    /// The link inside `context`, clamped to it.
    pub context_link: Range<usize>,
}

impl Source {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        // Keyed by the written form, as `decode` keys it, so a reload keeps a decode's sharing.
        let mut written: HashSet<Arc<str>, RandomState> = HashSet::default();
        let links = links(text)
            .into_iter()
            .filter_map(|link| {
                let Target::Note {
                    path,
                    md,
                    fragment,
                    style,
                } = Target::parse(&link)
                else {
                    return None;
                };
                let (context, context_link) =
                    context(text, link.range.start.get()..link.range.end.get());
                let path = written.get(path.as_str()).cloned().unwrap_or_else(|| {
                    let path: Arc<str> = Arc::from(path);
                    written.insert(Arc::clone(&path));
                    path
                });
                Some(SourceLink {
                    path,
                    fragment: fragment.map(String::into_boxed_str),
                    md,
                    style,
                    range: link.range,
                    context,
                    context_link,
                })
            })
            .collect();
        Self { links }
    }

    /// Each link to a note: its index into `links` and its target's stem key.
    pub(crate) fn note_stems(&self) -> impl Iterator<Item = (usize, Key)> + '_ {
        self.links
            .iter()
            .enumerate()
            .map(|(at, link)| (at, Key::stem(&link.path)))
    }
}

impl SourceLink {
    #[must_use]
    pub fn note(&self) -> NoteRef<'_> {
        NoteRef {
            path: &self.path,
            md: self.md,
            style: self.style,
        }
    }
}

fn context(text: &str, link: Range<usize>) -> (String, Range<usize>) {
    let line_start = text[..link.start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[link.start..]
        .find('\n')
        .map_or(text.len(), |at| link.start + at);
    let mut start = link.start.saturating_sub(BEFORE).max(line_start);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (start + CONTEXT).min(line_end);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let within = link.start - start..link.end.min(end).max(link.start) - start;
    (text[start..end].to_owned(), within)
}

#[cfg(test)]
mod tests;
