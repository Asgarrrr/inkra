//! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
//! quote's or list item's gutter (markdown blocks spec, D3; task boxes spec, D3). Offsets count
//! from the line's start, where both start.

mod gutter;
mod margin;
mod marks;

use std::ops::Range;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter, is_active};
use inkra_text::ByteOffset;

use super::runs::Shape;
use gutter::{Geometry, GutterLayout, Reveal};
use margin::{Hanging, Opening};

pub(super) use gutter::GutterCarry;
pub(super) use margin::heading_opening;
pub(super) use marks::{Align, Mark, Paint};

pub(super) enum Prefix {
    /// A heading's opening marker, left of the column.
    Margin(Hanging),
    /// A quote or list item prefix; the line's text starts at its width on every row.
    Gutter(GutterLayout),
}

impl Prefix {
    pub(super) fn range(&self) -> Range<usize> {
        match self {
            Self::Margin(hanging) => 0..hanging.len,
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

    /// A concealed item part's marks, the box last (task boxes spec, D3).
    pub(super) fn marks(&self) -> &[Mark] {
        match self {
            Self::Margin(_) => &[],
            Self::Gutter(gutter) => gutter.marks.as_deref().unwrap_or_default(),
        }
    }

    fn geometry(&self) -> Option<&Geometry> {
        match self {
            Self::Margin(_) => None,
            Self::Gutter(gutter) => Some(&gutter.geometry),
        }
    }

    /// The pieces that paint outside the text, and at which `x`; none while concealed.
    pub(super) fn shown(&self) -> impl Iterator<Item = (&ShapedLine, Pixels)> {
        let pieces = match self {
            Self::Margin(hanging) => [
                hanging
                    .shaped
                    .as_deref()
                    .map(|shaped| (shaped, -shaped.width())),
                None,
            ],
            Self::Gutter(gutter) => gutter.shown.as_deref().map_or([None, None], |shown| {
                shown
                    .pieces
                    .each_ref()
                    .map(|piece| piece.as_ref().map(|piece| (&piece.shaped, piece.x)))
            }),
        };
        pieces.into_iter().flatten()
    }
}

/// The conceals starting in a prefix are its own, and one the caret touches reveals it; returns the
/// prefix, relative to the line's start, and the conceals left for the text. A revealed marker
/// wider than `margin` gets no prefix and leaves its conceal to the text, so it shows inline
/// (live-preview spec, D13).
pub(super) fn line_prefix<'a, 'c>(
    gutter: Option<(&Gutter, &'a str)>,
    carry: &mut GutterCarry<'a>,
    opening: Option<Opening<'_>>,
    conceals: &'c [Conceal],
    selection: &Range<ByteOffset>,
    mut shape: impl FnMut(Shape<'_>) -> ShapedLine,
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
    // `>`s, then its item part's one conceal when it has a mark (task boxes spec, D2).
    debug_assert!(match gutter {
        Some((gutter, source)) => {
            let part = &source[gutter.quote_end() - gutter.range.start..];
            let item = gutter.item.is_some() && !part.trim_end_matches([' ', '\t']).is_empty();
            own.len() == gutter.quotes.len() + usize::from(item)
        }
        None =>
            own.len() <= 1
                && own
                    .iter()
                    .all(|conceal| Some(&conceal.marker) == opening.as_ref().map(|o| &o.range)),
    });
    let revealed = |own: &[Conceal]| own.iter().any(|conceal| !is_active(conceal, selection));
    if let Some((gutter, source)) = gutter {
        let (quotes, item) = own.split_at(gutter.quotes.len().min(own.len()));
        let reveal = Reveal {
            quote: revealed(quotes),
            item: revealed(item),
            split: gutter.quote_end() - gutter.range.start,
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
    let shaped = revealed(own).then(|| {
        Box::new(shape(Shape::Source {
            range: &range,
            source,
            size: font_size,
        }))
    });
    if shaped
        .as_ref()
        .is_some_and(|shaped| shaped.width() > margin)
    {
        return (None, conceals);
    }
    let len = range.end - range.start;
    (Some(Prefix::Margin(Hanging { len, shaped })), rest)
}

/// Where a line's text starts.
pub(super) fn indent(prefix: Option<&Prefix>) -> Pixels {
    prefix.map_or(px(0.), Prefix::indent)
}
