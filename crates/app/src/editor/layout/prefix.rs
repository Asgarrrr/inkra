//! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
//! quote's or task's gutter (markdown blocks spec, D3; task boxes spec, D3). Offsets count from the
//! line's start, where both start.

mod gutter;
mod margin;

use std::ops::Range;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter, is_active};
use inkra_text::ByteOffset;

use gutter::{Geometry, GutterLayout, Reveal};
use margin::{Hanging, Opening};

pub(super) use gutter::GutterCarry;
pub(super) use margin::heading_opening;

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

#[cfg(test)]
mod tests;
