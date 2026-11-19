//! A quote's or list item's gutter: the quote part's width and bars, measured once per text, the
//! item part's slots, and the revealed parts (markdown blocks spec, D3; task boxes spec, D3).

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::Gutter;

use super::Prefix;
use super::marks::{Mark, Marks};
use crate::editor::layout::runs::Shape;

pub(in crate::editor::layout) struct GutterLayout {
    /// From the line's start, where a gutter starts.
    pub(super) len: usize,
    /// `Some` only while a part is revealed (a caret touches a `>` or the item part, spec D2).
    pub(super) shown: Option<Box<Shown>>,
    pub(super) geometry: Geometry,
    /// The line's own item part's marks while it is concealed; never carried (task boxes spec,
    /// D3).
    pub(super) marks: Option<Box<[Mark]>>,
}

impl GutterLayout {
    /// A revealed part maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
    pub(super) fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.shown.as_ref().and_then(|shown| shown.index_for_x(x));
        index.unwrap_or(self.len)
    }
}

/// Which parts of a gutter a caret touches, and the byte, from the gutter's start, where its item
/// part starts: its length when it has none.
#[derive(Clone, Copy)]
pub(super) struct Reveal {
    pub(super) quote: bool,
    pub(super) item: bool,
    pub(super) split: usize,
}

/// The revealed parts of a gutter's text at the body size, each its own piece: the quote part at 0,
/// the item part ending at the text start (task boxes spec, D3).
pub(super) struct Shown {
    pub(super) pieces: [Option<Piece>; 2],
}

pub(super) struct Piece {
    pub(super) shaped: ShapedLine,
    /// Where its bytes start in the gutter's text.
    from: usize,
    pub(super) x: Pixels,
}

impl Shown {
    /// The revealed pieces of `shaped`, the whole raw text; a revealed item part widens `geometry`
    /// to the quote part plus its raw advance where that is wider.
    fn new(shaped: ShapedLine, reveal: Reveal, geometry: &mut Geometry) -> Self {
        let (quote, item) = match reveal.split {
            0 => (None, Some(shaped)),
            split if split == shaped.len() => (Some(shaped), None),
            split => {
                let (quote, item) = shaped.split_at(split);
                (Some(quote), Some(item))
            }
        };
        let quote = quote.filter(|_| reveal.quote).map(|shaped| Piece {
            shaped,
            from: 0,
            x: px(0.),
        });
        let item = item.filter(|_| reveal.item).map(|shaped| {
            geometry.width = geometry.width.max(geometry.quote_width + shaped.width());
            Piece {
                x: geometry.width - shaped.width(),
                shaped,
                from: reveal.split,
            }
        });
        Self {
            pieces: [quote, item],
        }
    }

    pub(super) fn x_for_index(&self, index: usize) -> Option<Pixels> {
        self.pieces.iter().flatten().find_map(|piece| {
            let within = index.checked_sub(piece.from)?;
            (within < piece.shaped.len()).then(|| piece.x + piece.shaped.x_for_index(within))
        })
    }

    /// Left of the column lands as on the first glyph of a piece at 0. Left of a later piece, as in
    /// the gap before a right-aligned item part, or at a piece's end, `None`: the caller lands on
    /// the text start.
    fn index_for_x(&self, x: Pixels) -> Option<usize> {
        self.pieces.iter().flatten().find_map(|piece| {
            let within = piece.shaped.closest_index_for_x(x - piece.x);
            (piece.x <= x.max(px(0.)) && within < piece.shaped.len()).then_some(piece.from + within)
        })
    }
}

/// A gutter's text start and bars, and the quote part's width, carried to lazy lines (spec D3).
/// Concealed, a marker line's text starts past its slots; revealed, past its raw item part where
/// that is wider.
#[derive(Clone, Default)]
pub(super) struct Geometry {
    pub(super) width: Pixels,
    /// Each `>`'s `x`, outermost first.
    pub(super) bars: Rc<[Pixels]>,
    /// Where the item part starts; `width` with no item part.
    quote_width: Pixels,
}

/// What gutters share within one `shape_document`: the geometry of the line above, built or
/// carried, which a lazy line takes, each distinct quote part's geometry, measured once, and the
/// marks' shapes.
pub(in crate::editor::layout) struct GutterCarry<'a> {
    carried: Geometry,
    /// Geometry depends only on the quote part's text at the body size (spec D3); most are `> ` or
    /// `> > `.
    quotes: HashMap<&'a str, Geometry>,
    marks: Marks<'a>,
    body_size: Pixels,
}

impl<'a> GutterCarry<'a> {
    pub(in crate::editor::layout) fn new(body_size: Pixels) -> Self {
        Self {
            carried: Geometry::default(),
            quotes: HashMap::new(),
            marks: Marks::default(),
            body_size,
        }
    }

    /// Every line laid out passes here, in order. A line with no gutter passes none on, so a lazy
    /// line reads only the line above, which the walk builds or carries first (D7). It carries the
    /// quote part only: an item part is its own line's (task boxes spec, D3).
    pub(in crate::editor::layout) fn pass(&mut self, prefix: Option<&Prefix>) {
        self.carried = prefix
            .and_then(Prefix::geometry)
            .map(Geometry::quote_part)
            .unwrap_or_default();
    }

    /// `source` is the gutter's text. A gutter holding all its `>` measures its quote part; a lazy
    /// one takes the carried geometry. The item part adds its slots; only a revealed part keeps a
    /// shape, to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        reveal: Reveal,
        mut shape: impl FnMut(Shape<'_>) -> ShapedLine,
    ) -> GutterLayout {
        let size = self.body_size;
        let shaped = (reveal.quote || reveal.item).then(|| {
            shape(Shape::Source {
                range: &gutter.range,
                source,
                size,
            })
        });
        let mut geometry = if gutter.quotes.len() == gutter.depth {
            let quote = &source[..reveal.split];
            if let Some(geometry) = self.quotes.get(quote) {
                geometry.clone()
            } else {
                let start = gutter.range.start;
                let shaped = (!gutter.quotes.is_empty()).then(|| {
                    shape(Shape::Source {
                        range: &(start..start + reveal.split),
                        source: quote,
                        size,
                    })
                });
                let geometry = Geometry::measure(gutter, shaped.as_ref());
                self.quotes.insert(quote, geometry.clone());
                geometry
            }
        } else {
            self.carried.clone()
        };
        let marks = gutter.item.as_ref().map(|item| {
            let part = &source[reveal.split..];
            let (marks, width) = self.marks.layout(item, part, geometry.quote_width, |text| {
                shape(Shape::Mark { text, size })
            });
            geometry.width = width;
            marks
        });
        GutterLayout {
            len: source.len(),
            shown: shaped.map(|shaped| Box::new(Shown::new(shaped, reveal, &mut geometry))),
            geometry,
            marks: marks.filter(|_| !reveal.item).map(Vec::into_boxed_slice),
        }
    }
}

impl Geometry {
    /// From the quote part's own shape; `None` with no `>`.
    fn measure(gutter: &Gutter, quote: Option<&ShapedLine>) -> Self {
        let start = gutter.range.start;
        let x = |index| quote.map_or(px(0.), |shaped| shaped.x_for_index(index));
        let quote_width = quote.map_or(px(0.), ShapedLine::width);
        Self {
            width: quote_width,
            bars: gutter
                .quotes
                .iter()
                .map(|quote| x(*quote - start))
                .collect(),
            quote_width,
        }
    }

    fn quote_part(&self) -> Self {
        Self {
            width: self.quote_width,
            bars: Rc::clone(&self.bars),
            quote_width: self.quote_width,
        }
    }
}
