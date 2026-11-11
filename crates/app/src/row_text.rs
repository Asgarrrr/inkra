//! One line of text with lit byte ranges, as quick open, vault search and backlinks draw it.

use std::ops::Range;

use gpui::SharedString;

use crate::one_line::one_line;

mod render;
#[cfg(test)]
mod tests;

pub(crate) use render::{name_row, styled};

/// One line of text and the ranges in it to light.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LitText {
    text: SharedString,
    ranges: Vec<Range<usize>>,
}

impl LitText {
    /// `one_line(text)`, with `ranges` made valid for it.
    pub(crate) fn line(text: &str, ranges: impl IntoIterator<Item = Range<usize>>) -> Self {
        // gpui panics on a highlight off a char boundary, in release too.
        let mut snapped: Vec<Range<usize>> = ranges
            .into_iter()
            .map(|range| text.floor_char_boundary(range.start)..text.ceil_char_boundary(range.end))
            .filter(|range| range.start < range.end)
            .collect();
        snapped.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(snapped.len());
        for range in snapped {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        Self {
            text: one_line(text).into_owned().into(),
            ranges: merged,
        }
    }

    /// As `line`, after the leading whitespace, cut no later than the first range's start.
    pub(crate) fn context(text: &str, ranges: impl IntoIterator<Item = Range<usize>>) -> Self {
        let ranges: Vec<Range<usize>> = ranges.into_iter().collect();
        let indent = text.len() - text.trim_start().len();
        let cut = text.floor_char_boundary(
            ranges
                .iter()
                .map(|range| range.start)
                .fold(indent, usize::min),
        );
        Self::line(&text[cut..], shifted(&ranges, cut))
    }

    /// The text before the first range, and the rest with its ranges.
    pub(crate) fn split_at_first(&self) -> (SharedString, Self) {
        let at = self
            .ranges
            .first()
            .map_or(self.text.len(), |range| range.start);
        let rest = Self {
            text: self.text[at..].to_owned().into(),
            ranges: shifted(&self.ranges, at).collect(),
        };
        (self.text[..at].to_owned().into(), rest)
    }

    pub(crate) fn text(&self) -> &SharedString {
        &self.text
    }

    pub(crate) fn ranges(&self) -> &[Range<usize>] {
        &self.ranges
    }
}

/// A note path as its name and its folder, each one line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PathText {
    pub(crate) name: LitText,
    pub(crate) folder: LitText,
}

impl PathText {
    /// `path` is `/`-separated with no extension; `ranges` index it.
    pub(crate) fn new(path: &str, ranges: &[Range<usize>]) -> Self {
        let stem_start = path.rfind('/').map_or(0, |slash| slash + 1);
        // The folder drops its last `/`; `line`'s clamp cuts a range to the folder.
        let folder_end = stem_start.saturating_sub(1);
        Self {
            name: LitText::line(&path[stem_start..], shifted(ranges, stem_start)),
            folder: LitText::line(&path[..folder_end], ranges.iter().cloned()),
        }
    }

    /// `folder/name`, or `name` at the root: the path one-lined.
    pub(crate) fn label(&self) -> SharedString {
        if self.folder.text.is_empty() {
            self.name.text.clone()
        } else {
            format!("{}/{}", self.folder.text, self.name.text).into()
        }
    }
}

/// `ranges` moved down by `by`; a range below it becomes empty and `line` drops it.
fn shifted(ranges: &[Range<usize>], by: usize) -> impl Iterator<Item = Range<usize>> + '_ {
    ranges
        .iter()
        .map(move |range| range.start.saturating_sub(by)..range.end.saturating_sub(by))
}
