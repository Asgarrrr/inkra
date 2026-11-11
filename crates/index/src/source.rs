use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use foldhash::fast::RandomState;
use inkra_markdown::links;
use inkra_text::ByteOffset;

use crate::key::Key;
use crate::target::{NoteRef, Style, Target};

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

#[cfg(test)]
mod tests;
