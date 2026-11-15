use gpui::{WrappedLine, px};
use inkra_markdown::{DisplayLine, DisplayOffset};
use inkra_text::Bias;
use unicode_segmentation::GraphemeCursor;

use super::line::Row;

/// One row per gpui row. Each wrap point snaps forward to the next grapheme
/// boundary of `shown`, so no caret lands inside a cluster (the glyphs before the snap still paint on the new row).
/// `len` is the line's, and `display` is over `0..len`.
pub(super) fn rows(
    shown: &str,
    wrapped: &WrappedLine,
    len: usize,
    display: &DisplayLine,
) -> Vec<Row> {
    let layout = &wrapped.unwrapped_layout;
    let at = |index: usize| {
        display
            .to_buffer(DisplayOffset::new(index), Bias::Left)
            .get()
    };

    let mut starts = vec![(0, px(0.))];
    for boundary in wrapped.wrap_boundaries() {
        let glyph = &layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix];
        let snapped = grapheme_at_or_after(shown, glyph.index);
        // Monotonic: two wraps snapping to one cluster end must not collide.
        let previous = starts.last().map_or(0, |(start, _)| *start);
        starts.push((snapped.max(previous), glyph.position.x));
    }

    let ends = starts
        .iter()
        .skip(1)
        .map(|(next, _)| Some(*next))
        .chain([None]);
    starts
        .iter()
        .zip(ends)
        .map(|(&(start, start_x), next)| Row {
            start: at(start),
            // The last row ends at the line's true end, past any hidden suffix (live-preview spec, D5).
            caret_end: next.map_or(len, |next| at(grapheme_before(shown, next).max(start))),
            start_x,
        })
        .collect()
}

fn grapheme_at_or_after(text: &str, index: usize) -> usize {
    let mut cursor = GraphemeCursor::new(index, text.len(), true);
    if cursor.is_boundary(text, 0).unwrap_or(true) {
        return index;
    }
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

fn grapheme_before(text: &str, index: usize) -> usize {
    GraphemeCursor::new(index, text.len(), true)
        .prev_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(0)
}
