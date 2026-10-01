//! Quote gutters: each quoted line's `>` prefix, found from the parse (markdown blocks spec, D1).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::parse::push;
use crate::{Conceal, Span, Styles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gutter {
    /// From the line's start through its last `>` and the one space or tab after it; empty on a lazy line.
    pub range: Range<ByteOffset>,
    /// Each `>` on the line, outermost first; fewer than `depth` on a lazy line.
    pub quotes: Vec<ByteOffset>,
    /// The leading quotes that contain the line, `>` or not.
    pub depth: usize,
}

/// One gutter per line inside a leading quote; `quotes` holds their ranges, without a trailing
/// `\n`, in start order.
pub(crate) fn gutters(text: &str, quotes: &[Range<usize>]) -> Vec<Gutter> {
    let mut gutters = Vec::new();
    // The end of each quote open on this line.
    let mut open: Vec<usize> = Vec::new();
    let mut rest = quotes;
    let mut start = 0;
    loop {
        if open.is_empty() {
            let Some(next) = rest.first() else {
                break;
            };
            start = text[..next.start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
        }
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |len| start + len);
        let entering = rest.iter().take_while(|quote| quote.start <= end).count();
        open.extend(rest[..entering].iter().map(|quote| quote.end));
        rest = &rest[entering..];
        gutters.push(gutter(text, start..end, open.len()));
        open.retain(|&quote_end| end < quote_end);
        start = end + 1;
    }
    gutters
}

/// One `>` per open quote, outermost first, each after up to three spaces (`CommonMark` 5.1);
/// the first one missing makes the line lazy from there on.
fn gutter(text: &str, line: Range<usize>, depth: usize) -> Gutter {
    let bytes = &text.as_bytes()[line.clone()];
    let mut end = 0;
    let mut quotes = Vec::new();
    for _ in 0..depth {
        let indent = bytes[end..]
            .iter()
            .take(3)
            .take_while(|&&byte| byte == b' ')
            .count();
        if bytes.get(end + indent) != Some(&b'>') {
            break;
        }
        quotes.push(ByteOffset(line.start + end + indent));
        end += indent + 1;
        end += usize::from(matches!(bytes.get(end), Some(b' ' | b'\t')));
    }
    Gutter {
        range: ByteOffset(line.start)..ByteOffset(line.start + end),
        quotes,
        depth,
    }
}

/// One conceal per `>`, all with the element from the gutter's start through its last `>`, so a
/// line's `>`s reveal together and a caret after the last one's space reveals none.
pub(crate) fn quote_conceals(gutters: &[Gutter]) -> impl Iterator<Item = Conceal> + '_ {
    gutters.iter().flat_map(|gutter| {
        let last = gutter
            .quotes
            .last()
            .map_or(gutter.range.start, |last| ByteOffset(last.0 + 1));
        let element = gutter.range.start..last;
        gutter.quotes.iter().map(move |&quote| Conceal {
            element: element.clone(),
            marker: quote..ByteOffset(quote.0 + 1),
            replacement: "",
        })
    })
}

/// A gutter's bytes are styled by the gutter alone: every span is cut around it, whatever
/// element emitted it. A leading quote's styles are `QUOTE`, its ancestors being quotes.
pub(crate) fn cut(text: &str, spans: &mut Vec<Span>, gutters: &[Gutter]) {
    if gutters.is_empty() {
        return;
    }
    // A gutter span must stay on one line, as every span does.
    debug_assert!(
        gutters
            .iter()
            .all(|gutter| !text[gutter.range.start.0..gutter.range.end.0].contains('\n')),
        "a gutter crosses a line"
    );
    let marker = Styles::QUOTE | Styles::MARKER;
    let mut gutters = gutters
        .iter()
        .map(|gutter| gutter.range.start.0..gutter.range.end.0)
        .peekable();
    // Everything before this byte is a gutter's or already re-emitted.
    let mut cut = 0;
    for span in std::mem::take(spans) {
        let range = span.range.start.0..span.range.end.0;
        while let Some(gutter) = gutters.next_if(|gutter| gutter.start < range.end) {
            push(spans, range.start.max(cut)..gutter.start, span.styles);
            cut = gutter.end;
            push(spans, gutter, marker);
        }
        push(spans, range.start.max(cut)..range.end, span.styles);
    }
    for gutter in gutters {
        push(spans, gutter, marker);
    }
}
