//! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
//! quote's gutter (markdown blocks spec, D3).

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::Gutter;
use inkra_text::ByteOffset;

pub(super) enum Prefix {
    /// A heading's opening marker, left of the column.
    Margin(Hanging),
    /// A quote prefix; the line's text starts at its width on every row.
    Gutter(GutterLayout),
}

impl Prefix {
    pub(super) fn range(&self) -> &Range<ByteOffset> {
        match self {
            Self::Margin(hanging) => &hanging.range,
            Self::Gutter(gutter) => &gutter.range,
        }
    }

    /// Where the text starts: a margin hangs left of it, a gutter pushes it right.
    pub(super) fn indent(&self) -> Pixels {
        match self {
            Self::Margin(_) => px(0.),
            Self::Gutter(gutter) => gutter.geometry.width,
        }
    }

    /// Row 0's `x` for an offset before `range().end`; `None` while concealed.
    pub(super) fn x_for_offset(&self, offset: ByteOffset) -> Option<Pixels> {
        match self {
            Self::Margin(hanging) => hanging.x_for_offset(offset),
            Self::Gutter(gutter) => gutter
                .shaped
                .as_ref()
                .map(|shaped| shaped.x_for_index(offset.0 - gutter.range.start.0)),
        }
    }

    /// Row 0's offset for an `x` left of `indent()`.
    pub(super) fn offset_for_x(&self, x: Pixels) -> ByteOffset {
        match self {
            Self::Margin(hanging) => hanging.offset_for_x(x),
            Self::Gutter(gutter) => gutter.offset_for_x(x),
        }
    }

    /// Only a gutter draws bars (spec D3).
    pub(super) fn bars(&self) -> &[Pixels] {
        match self {
            Self::Margin(_) => &[],
            Self::Gutter(gutter) => &gutter.geometry.bars,
        }
    }

    /// What paints outside the text, and at which `x`; `None` while concealed.
    pub(super) fn shown(&self) -> Option<(&ShapedLine, Pixels)> {
        match self {
            Self::Margin(hanging) => hanging
                .shaped
                .as_ref()
                .map(|shaped| (shaped, -shaped.width())),
            Self::Gutter(gutter) => gutter.shaped.as_ref().map(|shaped| (shaped, px(0.))),
        }
    }
}

pub(super) struct Hanging {
    pub(super) range: Range<ByteOffset>,
    /// `Some` only while revealed; painted left of the text and right-aligned to it.
    pub(super) shaped: Option<ShapedLine>,
}

impl Hanging {
    /// Negative, since the text starts at 0; `None` while concealed.
    pub(super) fn x_for_offset(&self, offset: ByteOffset) -> Option<Pixels> {
        let shaped = self.shaped.as_ref()?;
        Some(shaped.x_for_index(offset.0 - self.range.start.0) - shaped.width())
    }

    /// A concealed marker shows no glyph, so its whole margin lands on its start.
    pub(super) fn offset_for_x(&self, x: Pixels) -> ByteOffset {
        let index = self
            .shaped
            .as_ref()
            .map_or(0, |shaped| shaped.closest_index_for_x(x + shaped.width()));
        ByteOffset(self.range.start.0 + index)
    }
}

/// Where a line's text starts.
pub(super) fn indent(prefix: Option<&Prefix>) -> Pixels {
    prefix.map_or(px(0.), Prefix::indent)
}

pub(super) struct GutterLayout {
    pub(super) range: Range<ByteOffset>,
    /// `Some` only while revealed (a caret touches a `>`, spec D2): the gutter's own text at the
    /// body size, painted at `x = 0`.
    pub(super) shaped: Option<ShapedLine>,
    pub(super) geometry: Geometry,
}

impl GutterLayout {
    /// A revealed gutter maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
    fn offset_for_x(&self, x: Pixels) -> ByteOffset {
        self.shaped.as_ref().map_or(self.range.end, |shaped| {
            ByteOffset(self.range.start.0 + shaped.closest_index_for_x(x))
        })
    }
}

/// A gutter's text start and bars, carried to lazy lines (spec D3).
#[derive(Clone, Default)]
pub(super) struct Geometry {
    width: Pixels,
    /// Each `>`'s `x`, outermost first.
    bars: Rc<[Pixels]>,
}

/// What gutters share within one `shape_document`: the geometry the last one holding all its `>`
/// measured, carried to lazy lines, and each distinct gutter text's geometry, measured once.
pub(super) struct GutterCarry<'a> {
    carried: Geometry,
    /// Geometry depends only on the gutter's text at the body size (spec D3); most are `> ` or `> > `.
    measured: HashMap<&'a str, Geometry>,
    body_size: Pixels,
}

impl<'a> GutterCarry<'a> {
    pub(super) fn new(body_size: Pixels) -> Self {
        Self {
            carried: Geometry::default(),
            measured: HashMap::new(),
            body_size,
        }
    }

    /// `source` is the gutter's text. A gutter holding all its `>` measures itself; a lazy one takes
    /// the last measured geometry. Only a revealed gutter keeps a shape, to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        revealed: bool,
        mut shape: impl FnMut(&Range<ByteOffset>, Pixels) -> ShapedLine,
    ) -> GutterLayout {
        let body_size = self.body_size;
        let shaped = revealed.then(|| shape(&gutter.range, body_size));
        if gutter.quotes.len() == gutter.depth {
            self.carried = if let Some(geometry) = self.measured.get(source) {
                geometry.clone()
            } else {
                let geometry = match &shaped {
                    Some(shaped) => Geometry::measure(gutter, shaped),
                    None => Geometry::measure(gutter, &shape(&gutter.range, body_size)),
                };
                self.measured.insert(source, geometry.clone());
                geometry
            };
        }
        GutterLayout {
            range: gutter.range.clone(),
            shaped,
            geometry: self.carried.clone(),
        }
    }
}

impl Geometry {
    fn measure(gutter: &Gutter, shaped: &ShapedLine) -> Self {
        let start = gutter.range.start.0;
        Self {
            width: shaped.width(),
            bars: gutter
                .quotes
                .iter()
                .map(|quote| shaped.x_for_index(quote.0 - start))
                .collect(),
        }
    }
}
