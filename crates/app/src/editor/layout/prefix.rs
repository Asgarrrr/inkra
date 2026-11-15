//! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
//! quote's gutter (markdown blocks spec, D3). Offsets count from the line's start, where both start.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter, Heading, is_active};
use inkra_text::ByteOffset;

pub(super) enum Prefix {
    /// A heading's opening marker, left of the column.
    Margin(Hanging),
    /// A quote prefix; the line's text starts at its width on every row.
    Gutter(GutterLayout),
}

impl Prefix {
    pub(super) fn range(&self) -> &Range<usize> {
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
    pub(super) fn x_for_offset(&self, offset: usize) -> Option<Pixels> {
        match self {
            Self::Margin(hanging) => hanging.x_for_offset(offset),
            Self::Gutter(gutter) => gutter
                .shaped
                .as_ref()
                .map(|shaped| shaped.x_for_index(offset - gutter.range.start)),
        }
    }

    /// Row 0's offset for an `x` left of `indent()`.
    pub(super) fn offset_for_x(&self, x: Pixels) -> usize {
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

    fn geometry(&self) -> Option<&Geometry> {
        match self {
            Self::Margin(_) => None,
            Self::Gutter(gutter) => Some(&gutter.geometry),
        }
    }

    /// What paints outside the text, and at which `x`; `None` while concealed.
    pub(super) fn shown(&self) -> Option<(&ShapedLine, Pixels)> {
        match self {
            Self::Margin(hanging) => hanging
                .shaped
                .as_deref()
                .map(|shaped| (shaped, -shaped.width())),
            Self::Gutter(gutter) => gutter.shaped.as_deref().map(|shaped| (shaped, px(0.))),
        }
    }
}

pub(super) struct Hanging {
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
pub(super) struct Opening {
    range: Range<ByteOffset>,
    font_size: Pixels,
    margin: Pixels,
}

/// The marker that opens the line as a margin. Inside a container it keeps its inline conceal:
/// no margin exists there.
pub(super) fn heading_opening(
    heading: Option<&Heading>,
    range: &Range<ByteOffset>,
    font_size: Pixels,
    margin: Pixels,
) -> Option<Opening> {
    heading
        .and_then(|heading| heading.marker.clone())
        .filter(|marker| marker.start == range.start)
        .map(|range| Opening {
            range,
            font_size,
            margin,
        })
}

/// The conceals starting in a prefix are its own, and one the caret touches reveals it; returns the
/// prefix, relative to `start`, the line's, and the conceals left for the text. A revealed marker
/// wider than `margin` gets no prefix and leaves its conceal to the text, so it shows inline
/// (live-preview spec, D13).
pub(super) fn line_prefix<'a, 'c>(
    gutter: Option<(&Gutter, &'a str)>,
    start: ByteOffset,
    carry: &mut GutterCarry<'a>,
    opening: Option<Opening>,
    conceals: &'c [Conceal],
    selection: &Range<ByteOffset>,
    mut shape: impl FnMut(&Range<ByteOffset>, Pixels) -> ShapedLine,
) -> (Option<Prefix>, &'c [Conceal]) {
    let own = gutter
        .map(|(gutter, _)| &gutter.range)
        .or(opening.as_ref().map(|opening| &opening.range))
        .map_or(0, |prefix| {
            conceals.partition_point(|conceal| conceal.marker.start < prefix.end)
        });
    let (own, rest) = conceals.split_at(own);
    // A gap conceal skips a line's leading `[ \t>]*` and a heading's own marker is the only conceal
    // before `opening.end`, so every conceal before the prefix's end is the prefix's own.
    debug_assert!(match gutter {
        Some((gutter, _)) => own.len() == gutter.quotes.len(),
        None =>
            own.len() <= 1
                && own
                    .iter()
                    .all(|conceal| Some(&conceal.marker) == opening.as_ref().map(|o| &o.range)),
    });
    let revealed = own.iter().any(|conceal| !is_active(conceal, selection));
    if let Some((gutter, source)) = gutter {
        let layout = carry.layout(gutter, source, start, revealed, &mut shape);
        return (Some(Prefix::Gutter(layout)), rest);
    }
    let Some(Opening {
        range,
        font_size,
        margin,
    }) = opening
    else {
        return (None, rest);
    };
    let shaped = revealed.then(|| Box::new(shape(&range, font_size)));
    if shaped
        .as_ref()
        .is_some_and(|shaped| shaped.width() > margin)
    {
        return (None, conceals);
    }
    let range = range.start - start..range.end - start;
    (Some(Prefix::Margin(Hanging { range, shaped })), rest)
}

/// Where a line's text starts.
pub(super) fn indent(prefix: Option<&Prefix>) -> Pixels {
    prefix.map_or(px(0.), Prefix::indent)
}

pub(super) struct GutterLayout {
    pub(super) range: Range<usize>,
    /// `Some` only while revealed (a caret touches a `>`, spec D2): the gutter's own text at the
    /// body size, painted at `x = 0`.
    pub(super) shaped: Option<Box<ShapedLine>>,
    pub(super) geometry: Geometry,
}

impl GutterLayout {
    /// A revealed gutter maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
    fn offset_for_x(&self, x: Pixels) -> usize {
        self.shaped.as_ref().map_or(self.range.end, |shaped| {
            self.range.start + shaped.closest_index_for_x(x)
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

/// What gutters share within one `shape_document`: the geometry of the line above, built or
/// carried, which a lazy line takes, and each distinct gutter text's geometry, measured once.
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

    /// Every line laid out passes here, in order. A line with no gutter passes none on, so a lazy
    /// line reads only the line above, which the walk builds or carries first (D7).
    pub(super) fn pass(&mut self, prefix: Option<&Prefix>) {
        self.carried = prefix
            .and_then(Prefix::geometry)
            .cloned()
            .unwrap_or_default();
    }

    /// `source` is the gutter's text, and `start` its line's. A gutter holding all its `>` measures
    /// itself; a lazy one takes the carried geometry. Only a revealed gutter keeps a shape, to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        start: ByteOffset,
        revealed: bool,
        mut shape: impl FnMut(&Range<ByteOffset>, Pixels) -> ShapedLine,
    ) -> GutterLayout {
        let body_size = self.body_size;
        let shaped = revealed.then(|| Box::new(shape(&gutter.range, body_size)));
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
            range: gutter.range.start - start..gutter.range.end - start,
            shaped,
            geometry: self.carried.clone(),
        }
    }
}

impl Geometry {
    fn measure(gutter: &Gutter, shaped: &ShapedLine) -> Self {
        let start = gutter.range.start;
        Self {
            width: shaped.width(),
            bars: gutter
                .quotes
                .iter()
                .map(|quote| shaped.x_for_index(*quote - start))
                .collect(),
        }
    }
}
