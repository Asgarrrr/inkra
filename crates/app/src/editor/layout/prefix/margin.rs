//! A heading's hanging marker, left of the column (live-preview spec, D12).

use std::ops::Range;

use gpui::{Pixels, ShapedLine};
use inkra_markdown::Heading;
use inkra_text::ByteOffset;

pub(in crate::editor::layout) struct Hanging {
    pub(super) range: Range<usize>,
    /// `Some` only while revealed; painted left of the text and right-aligned to it.
    pub(super) shaped: Option<Box<ShapedLine>>,
}

impl Hanging {
    /// Negative, since the text starts at 0; `None` while concealed.
    pub(super) fn x_for_offset(&self, offset: usize) -> Option<Pixels> {
        let shaped = self.shaped.as_ref()?;
        Some(shaped.x_for_index(offset - self.range.start) - shaped.width())
    }

    /// A concealed marker shows no glyph, so its whole margin lands on its start.
    pub(super) fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self
            .shaped
            .as_ref()
            .map_or(0, |shaped| shaped.closest_index_for_x(x + shaped.width()));
        self.range.start + index
    }
}

/// A heading's marker that opens its line, at the heading's size, with the room left of the text.
pub(in crate::editor::layout) struct Opening<'a> {
    pub(super) range: Range<ByteOffset>,
    pub(super) source: &'a str,
    pub(super) font_size: Pixels,
    pub(super) margin: Pixels,
}

/// The marker that opens the line at `range`, whose text is `line`, as a margin. Inside a container
/// it keeps its inline conceal: no margin exists there.
pub(in crate::editor::layout) fn heading_opening<'a>(
    heading: Option<&Heading>,
    range: &Range<ByteOffset>,
    line: &'a str,
    font_size: Pixels,
    margin: Pixels,
) -> Option<Opening<'a>> {
    heading
        .and_then(|heading| heading.marker.clone())
        .filter(|marker| marker.start == range.start)
        .map(|marker| Opening {
            source: &line[..marker.end - marker.start],
            range: marker,
            font_size,
            margin,
        })
}
