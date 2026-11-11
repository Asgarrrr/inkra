//! The selection's quads: one per row a selection touches.

use std::ops::Range;

use gpui::{Bounds, PaintQuad, fill, point};
use inkra_text::ByteOffset;

use super::{DocumentLayout, NEWLINE_STUB};

impl DocumentLayout {
    /// One quad per row touched. Four cases: a one-row selection is both first
    /// and last, and a row ending in a wrap gets no [`NEWLINE_STUB`] gap since a wrap isn't a character.
    /// A scrollable block's rows stop at its background.
    pub(crate) fn selection_quads(&self, selection: &Range<ByteOffset>) -> Vec<PaintQuad> {
        let first = self.row_for_offset(selection.start);
        let last = self.row_for_offset(selection.end);

        (first..=last)
            .filter_map(|row| {
                let (line, within) = self.locate(row);
                let left = if row == first {
                    self.x_on_row(line, within, selection.start)
                } else {
                    self.x_on_row(line, within, line.rows[within].start)
                };
                let right = if row == last {
                    self.x_on_row(line, within, selection.end)
                } else if within + 1 == line.rows.len() {
                    self.row_end_x(line, within) + line.row_height * NEWLINE_STUB
                } else {
                    self.row_end_x(line, within)
                };
                let (left, right) = match line.block.and_then(|block| self.block_clip(block)) {
                    Some(clip) => clip.span(left, right, line.row_height)?,
                    None => (left, right),
                };
                let top = self.top_for_row(row);
                Some(fill(
                    Bounds::from_corners(point(left, top), point(right, top + line.row_height)),
                    self.theme.colors.selection,
                ))
            })
            .collect()
    }
}
