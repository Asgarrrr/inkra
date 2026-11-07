use std::collections::HashMap;

use gpui::{Bounds, Pixels, Point, ShapedLine, SharedString, TextStyle, Window, point, px};
use inkra_markdown::CodeBlock;
use inkra_text::ByteOffset;

use super::{DocumentLayout, LineLayout};
use crate::theme::{self, Theme};

#[cfg(test)]
thread_local! {
    static LABELS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Label texts shaped so far on this thread; a repeated language shapes once per layout.
#[cfg(test)]
pub(crate) fn label_shape_count() -> usize {
    LABELS.with(std::cell::Cell::get)
}

/// What one code block paints (code blocks spec, C2-C3).
pub(super) struct BlockLayout {
    /// `CodeBlock.range.start`, which keys its scroll.
    pub(super) start: ByteOffset,
    /// How far its text scrolls: zero when every line fits.
    pub(super) max: Pixels,
    pub(super) background: Bounds<Pixels>,
    pub(super) label: Option<(Point<Pixels>, ShapedLine)>,
}

/// Shaped label texts, each distinct one once per `shape_document` call.
pub(super) struct Labels<'a> {
    style: TextStyle,
    shaped: HashMap<&'a str, ShapedLine>,
}

impl<'a> Labels<'a> {
    /// `style` is the layout's body style; the label takes `text_muted`.
    pub(super) fn new(style: &TextStyle, theme: &Theme) -> Self {
        Self {
            style: TextStyle {
                color: theme.colors.text_muted.into(),
                ..style.clone()
            },
            shaped: HashMap::new(),
        }
    }

    /// Shapes each distinct text once per layout.
    pub(super) fn shape(&mut self, text: &'a str, window: &mut Window) -> ShapedLine {
        self.shaped
            .entry(text)
            .or_insert_with(|| {
                #[cfg(test)]
                LABELS.with(|labels| labels.set(labels.get() + 1));
                window.text_system().shape_line(
                    SharedString::from(text.to_owned()),
                    theme::CODE_BLOCK_LABEL_SIZE,
                    &[self.style.to_run(text.len())],
                    None,
                )
            })
            .clone()
    }
}

/// Builds the blocks' layouts from the lines `shape_document` visits in order.
pub(super) struct Blocks {
    done: Vec<BlockLayout>,
    open: Option<Building>,
    column: Option<Pixels>,
}

struct Building {
    start: ByteOffset,
    label: Option<ShapedLine>,
    top: Pixels,
    left: Pixels,
    widest: Pixels,
    bottom: Pixels,
}

impl Blocks {
    /// `column` is the wrap width, `None` off the editor: there the widest line stands in.
    pub(super) fn new(column: Option<Pixels>) -> Self {
        Self {
            done: Vec::new(),
            open: None,
            column,
        }
    }

    /// The index `block`'s layout takes in `finish`'s list, for the line it holds.
    pub(super) fn index_of(&self, block: Option<&CodeBlock>) -> Option<usize> {
        let block = block?;
        let closes_first = self
            .open
            .as_ref()
            .is_some_and(|open| open.start != block.range.start);
        Some(self.done.len() + usize::from(closes_first))
    }

    /// `block` holds `line`, from the cursor `line_metrics` reads; `bottom` is the line's last row's
    /// bottom. `label` is read on the line that opens the block.
    pub(super) fn line(
        &mut self,
        block: Option<&CodeBlock>,
        label: Option<ShapedLine>,
        line: &LineLayout,
        bottom: Pixels,
    ) {
        if self
            .open
            .as_ref()
            .is_some_and(|open| block.is_none_or(|block| block.range.start != open.start))
        {
            self.close();
        }
        if let Some(block) = block
            && self.open.is_none()
        {
            self.open = Some(Building {
                start: block.range.start,
                label,
                top: line.top,
                left: DocumentLayout::content_x(line, 0, block.range.start)
                    - theme::CODE_BLOCK_OUTSET,
                widest: px(0.),
                bottom,
            });
        }
        if let Some(open) = &mut self.open {
            open.widest = open.widest.max(line.indent() + line.shape.wrapped.width());
            open.bottom = bottom;
        }
    }

    /// Closes the block a note ends inside.
    pub(super) fn finish(mut self) -> Vec<BlockLayout> {
        self.close();
        self.done
    }

    fn close(&mut self) {
        if let Some(open) = self.open.take() {
            let right = self.column.unwrap_or(open.widest) + theme::CODE_BLOCK_OUTSET;
            let max = match self.column {
                Some(column) if open.widest > column => {
                    open.widest - column + theme::CODE_BLOCK_SCROLL_MARGIN
                }
                _ => px(0.),
            };
            self.done.push(BlockLayout {
                start: open.start,
                max,
                background: Bounds::from_corners(
                    point(open.left, open.top),
                    point(right, open.bottom),
                ),
                label: open.label.map(|shaped| {
                    let at = point(right - theme::CODE_BLOCK_OUTSET - shaped.width(), open.top);
                    (at, shaped)
                }),
            });
        }
    }
}
