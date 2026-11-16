//! One line's layout, relative to its start, and where one layout places it (incremental layout
//! spec, D3).

use std::rc::Rc;

use gpui::{Bounds, Pixels, ShapedLine, SharedString, TextRun, Window, WrappedLine, point, size};
use inkra_markdown::DisplayLine;
use inkra_text::ByteOffset;

use super::prefix::{self, Boxed, Prefix};
use crate::theme::TASK_BOX_SIZE;

#[cfg(test)]
thread_local! {
    static LINES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Lines handed to `shape_text` so far on this thread.
#[cfg(test)]
pub(crate) fn line_shape_count() -> usize {
    LINES.with(std::cell::Cell::get)
}

/// One line's `shape_text` output and every argument it took; a later layout reuses it only when all match.
pub(super) struct LineShape {
    pub(super) wrapped: WrappedLine,
    text: SharedString,
    font_size: Pixels,
    pub(super) runs: Vec<TextRun>,
    wrap: Option<Pixels>,
}

impl LineShape {
    pub(super) fn new(
        text: &str,
        font_size: Pixels,
        runs: Vec<TextRun>,
        wrap: Option<Pixels>,
        start: ByteOffset,
        window: &mut Window,
    ) -> Self {
        #[cfg(test)]
        LINES.with(|lines| lines.set(lines.get() + 1));
        let text = SharedString::from(text.to_owned());
        let wrapped = window
            .text_system()
            .shape_text(text.clone(), font_size, &runs, wrap, None)
            .map_err(|err| log::warn!("failed to shape line at {start}: {err}"))
            .ok()
            .and_then(|mut shaped| shaped.pop())
            .unwrap_or_default();
        Self {
            wrapped,
            text,
            font_size,
            runs,
            wrap,
        }
    }

    pub(super) fn shaped_from(
        &self,
        text: &str,
        font_size: Pixels,
        runs: &[TextRun],
        wrap: Option<Pixels>,
    ) -> bool {
        self.font_size == font_size
            && self.wrap == wrap
            && self.text.as_ref() == text
            && self.runs == runs
    }

    #[cfg(test)]
    pub(super) fn same_args(&self, other: &Self) -> bool {
        other.shaped_from(&self.text, self.font_size, &self.runs, self.wrap)
    }
}

/// Holds nothing an edit above the line can move, so a later layout carries it whole.
pub(super) struct LineLayout {
    pub(super) shape: Rc<LineShape>,
    /// Bytes, without the `\n`.
    pub(super) len: usize,
    /// Over `0..len`.
    pub(super) display: DisplayLine,
    pub(super) rows: Vec<Row>,
    pub(super) row_height: Pixels,
    /// Kept out of the shaped text in both states, so no reveal re-wraps the line.
    pub(super) prefix: Option<Prefix>,
    /// The language a block's opening line shows at its right.
    pub(super) label: Option<Rc<ShapedLine>>,
}

// Every keystroke collects one per line, so an inline prefix shape cost milliseconds (incremental layout spec, D4).
const _: () = assert!(size_of::<LineLayout>() < 256);

impl LineLayout {
    pub(super) fn indent(&self) -> Pixels {
        prefix::indent(self.prefix.as_ref())
    }

    /// The square a concealed task paints and a click hits, from the line's origin: centred on its
    /// checkbox and on row 0 (task boxes spec, D4).
    pub(super) fn task_box(&self) -> Option<(Bounds<Pixels>, &Boxed)> {
        let (checkbox, boxed) = self.prefix.as_ref()?.task_box()?;
        let centre = point((checkbox.start + checkbox.end) / 2., self.row_height / 2.);
        let square = Bounds::centered_at(centre, size(TASK_BOX_SIZE, TASK_BOX_SIZE));
        Some((square, boxed))
    }
}

/// Where a line lands in one layout: what an edit above it moves.
pub(super) struct Placed {
    pub(super) start: ByteOffset,
    pub(super) top: Pixels,
    pub(super) first_row: usize,
    /// The index in `code_blocks` of the block holding this line.
    pub(super) block: Option<usize>,
    pub(super) line: Rc<LineLayout>,
}

impl Placed {
    pub(super) fn end(&self) -> ByteOffset {
        self.start + self.line.len
    }

    /// `offset` from the line's start, clamped to the line.
    pub(super) fn within(&self, offset: ByteOffset) -> usize {
        offset.clamp(self.start, self.end()) - self.start
    }
}

/// A wrap offset belongs to the next row, never the earlier one's `caret_end`
/// (Zed's rule, `wrap_map.rs:969-981`) — this is what lets the caret skip affinity.
/// Its offsets count from the line's start.
pub(super) struct Row {
    /// On a grapheme boundary, even where gpui broke inside a cluster.
    pub(super) start: usize,
    pub(super) caret_end: usize,
    pub(super) start_x: Pixels,
}
