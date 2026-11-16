//! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
//! quote's or task's gutter (markdown blocks spec, D3; task boxes spec, D3). Offsets count from the
//! line's start, where both start.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter, Heading, is_active};
use inkra_text::ByteOffset;

pub(super) enum Prefix {
    /// A heading's opening marker, left of the column.
    Margin(Hanging),
    /// A quote or task prefix; the line's text starts at its width on every row.
    Gutter(GutterLayout),
}

impl Prefix {
    pub(super) fn range(&self) -> Range<usize> {
        match self {
            Self::Margin(hanging) => hanging.range.clone(),
            Self::Gutter(gutter) => 0..gutter.len,
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
                .shown
                .as_ref()
                .and_then(|shown| shown.x_for_index(offset)),
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
            Self::Gutter(gutter) => gutter
                .shown
                .as_deref()
                .map(|shown| (&shown.shaped, shown.x)),
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
pub(super) struct Opening<'a> {
    range: Range<ByteOffset>,
    source: &'a str,
    font_size: Pixels,
    margin: Pixels,
}

/// The marker that opens the line at `range`, whose text is `line`, as a margin. Inside a container
/// it keeps its inline conceal: no margin exists there.
pub(super) fn heading_opening<'a>(
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

/// The conceals starting in a prefix are its own, and one the caret touches reveals it; returns the
/// prefix, relative to `start`, the line's, and the conceals left for the text. A revealed marker
/// wider than `margin` gets no prefix and leaves its conceal to the text, so it shows inline
/// (live-preview spec, D13). `shape` shapes a range from the text it is given, of the range's length.
pub(super) fn line_prefix<'a, 'c>(
    gutter: Option<(&Gutter, &'a str)>,
    start: ByteOffset,
    carry: &mut GutterCarry<'a>,
    opening: Option<Opening<'_>>,
    conceals: &'c [Conceal],
    selection: &Range<ByteOffset>,
    mut shape: impl FnMut(&Range<ByteOffset>, &str, Pixels) -> ShapedLine,
) -> (Option<Prefix>, &'c [Conceal]) {
    let own = gutter
        .map(|(gutter, _)| &gutter.range)
        .or(opening.as_ref().map(|opening| &opening.range))
        .map_or(0, |prefix| {
            conceals.partition_point(|conceal| conceal.marker.start < prefix.end)
        });
    let (own, rest) = conceals.split_at(own);
    // A gap conceal skips a line's leading `[ \t>]*` and a heading's own marker is the only conceal
    // before `opening.end`, so every conceal before the prefix's end is the prefix's own: a gutter's
    // `>`s, then its task part, last since every `>` precedes the bullet.
    debug_assert!(match gutter {
        Some((gutter, _)) => own.len() == gutter.quotes.len() + usize::from(gutter.task.is_some()),
        None =>
            own.len() <= 1
                && own
                    .iter()
                    .all(|conceal| Some(&conceal.marker) == opening.as_ref().map(|o| &o.range)),
    });
    let revealed = |own: &[Conceal]| own.iter().any(|conceal| !is_active(conceal, selection));
    if let Some((gutter, source)) = gutter {
        let (quotes, task) = own.split_at(gutter.quotes.len().min(own.len()));
        let reveal = Reveal {
            quote: revealed(quotes),
            task: revealed(task),
            split: gutter.quote_end - gutter.range.start,
        };
        let layout = carry.layout(gutter, source, reveal, &mut shape);
        return (Some(Prefix::Gutter(layout)), rest);
    }
    let Some(Opening {
        range,
        source,
        font_size,
        margin,
    }) = opening
    else {
        return (None, rest);
    };
    let shaped = revealed(own).then(|| Box::new(shape(&range, source, font_size)));
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
    /// From the line's start, where a gutter starts.
    pub(super) len: usize,
    /// `Some` only while a part is revealed (a caret touches a `>` or the task part, spec D2).
    pub(super) shown: Option<Box<Shown>>,
    pub(super) geometry: Geometry,
}

impl GutterLayout {
    /// A revealed part maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
    fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.shown.as_ref().and_then(|shown| shown.index_for_x(x));
        index.unwrap_or(self.len)
    }
}

/// Which parts of a gutter a caret touches, and the byte, from the gutter's start, where its task
/// part starts: its length when it has none.
#[derive(Clone, Copy)]
pub(super) struct Reveal {
    quote: bool,
    task: bool,
    split: usize,
}

/// The revealed part of a gutter's text at the body size, or the whole text when both parts are: its
/// bytes from `from`, painted at `x`, where they stand in the whole (task boxes spec, D3).
pub(super) struct Shown {
    shaped: ShapedLine,
    from: usize,
    x: Pixels,
}

impl Shown {
    /// One whole shape of the gutter's text, split at the task part when only one part shows.
    fn new(shaped: ShapedLine, reveal: Reveal) -> Self {
        let left = reveal.quote || reveal.split == 0;
        let right = reveal.task || reveal.split == shaped.len();
        if left && right {
            return Self {
                shaped,
                from: 0,
                x: px(0.),
            };
        }
        let (quote, task) = shaped.split_at(reveal.split);
        if left {
            Self {
                shaped: quote,
                from: 0,
                x: px(0.),
            }
        } else {
            Self {
                x: quote.width(),
                shaped: task,
                from: reveal.split,
            }
        }
    }

    fn x_for_index(&self, index: usize) -> Option<Pixels> {
        let within = index.checked_sub(self.from)?;
        (within < self.shaped.len()).then(|| self.x + self.shaped.x_for_index(within))
    }

    /// Left of the column lands as on the first glyph, as before the split; left of the shown part,
    /// or at its end, `None`, as `x_for_index` reads it.
    fn index_for_x(&self, x: Pixels) -> Option<usize> {
        let within = self.shaped.closest_index_for_x(x - self.x);
        (self.x <= x.max(px(0.)) && within < self.shaped.len()).then_some(self.from + within)
    }
}

/// A gutter's text start and bars, and the quote part's width, carried to lazy lines (spec D3).
#[derive(Clone, Default)]
pub(super) struct Geometry {
    width: Pixels,
    /// Each `>`'s `x`, outermost first.
    bars: Rc<[Pixels]>,
    /// Where the task part starts; `width` with no task.
    quote_width: Pixels,
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
    /// line reads only the line above, which the walk builds or carries first (D7). It carries the
    /// quote part only: a task's marker is its own line's (task boxes spec, D3).
    pub(super) fn pass(&mut self, prefix: Option<&Prefix>) {
        self.carried = prefix
            .and_then(Prefix::geometry)
            .map(Geometry::quote_part)
            .unwrap_or_default();
    }

    /// `source` is the gutter's text. A gutter holding all its `>` measures
    /// itself; a lazy one takes the carried geometry. Only a revealed part keeps a shape, to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        reveal: Reveal,
        mut shape: impl FnMut(&Range<ByteOffset>, &str, Pixels) -> ShapedLine,
    ) -> GutterLayout {
        let body_size = self.body_size;
        let shaped = (reveal.quote || reveal.task).then(|| shape(&gutter.range, source, body_size));
        let geometry = if gutter.quotes.len() == gutter.depth {
            if let Some(geometry) = self.measured.get(source) {
                geometry.clone()
            } else {
                let measured = measured_text(source, gutter);
                let geometry = if let (Some(shaped), Cow::Borrowed(_)) = (&shaped, &measured) {
                    Geometry::measure(gutter, shaped, reveal.split)
                } else {
                    let shaped = shape(&gutter.range, &measured, body_size);
                    Geometry::measure(gutter, &shaped, reveal.split)
                };
                self.measured.insert(source, geometry.clone());
                geometry
            }
        } else {
            self.carried.clone()
        };
        GutterLayout {
            len: source.len(),
            shown: shaped.map(|shaped| Box::new(Shown::new(shaped, reveal))),
            geometry,
        }
    }
}

/// What a gutter measures: its text with its box's middle byte an `X`, the widest state, so every
/// state paints inside one width and a toggle never changes it (task boxes spec, D3, C).
fn measured_text<'s>(source: &'s str, gutter: &Gutter) -> Cow<'s, str> {
    match &gutter.task {
        Some(task) => {
            let middle = task.checkbox.start - gutter.range.start + 1;
            let mut text = source.to_owned();
            text.replace_range(middle..=middle, "X");
            Cow::Owned(text)
        }
        None => Cow::Borrowed(source),
    }
}

impl Geometry {
    fn measure(gutter: &Gutter, shaped: &ShapedLine, split: usize) -> Self {
        let start = gutter.range.start;
        Self {
            width: shaped.width(),
            bars: gutter
                .quotes
                .iter()
                .map(|quote| shaped.x_for_index(*quote - start))
                .collect(),
            quote_width: shaped.x_for_index(split),
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

#[cfg(test)]
mod tests;
