//! The walk over the lines gutter items hold: each one's item part, after its quote part (task
//! boxes spec, D11).

use std::iter::Peekable;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::{Items, LeafText, Record, column_after};
use crate::gutters::{Gutter, WidthKey, gutter_at};
use crate::spans::line_end;

/// A gutter item holding the lines the walk visits.
pub(super) struct Held<'r> {
    pub(super) record: &'r Record,
    /// The column after its marker line's item part (D11).
    pub(super) hang: usize,
}

/// The walk's cursors, each moving forward only: the quote lines not yet given out, the leaf texts
/// and the code blocks not yet passed, the gutter items holding the line, outermost first, and the
/// gutters and width keys so far.
pub(super) struct Walk<'t, 'r, B: Iterator<Item = Range<usize>>> {
    pub(super) text: &'t str,
    pub(super) leaf_texts: Peekable<std::slice::Iter<'r, LeafText>>,
    pub(super) quote_lines: Peekable<std::vec::IntoIter<Gutter>>,
    pub(super) blocks: Peekable<B>,
    pub(super) held: Vec<Held<'r>>,
    pub(super) out: Vec<Gutter>,
    pub(super) keys: Vec<WidthKey>,
}

impl<B: Iterator<Item = Range<usize>>> Walk<'_, '_, B> {
    /// Drops the items that end by `at`.
    pub(super) fn release(&mut self, at: usize) {
        while self.held.last().is_some_and(|held| held.record.end <= at) {
            self.held.pop();
        }
    }

    /// Gives out the quote lines before the line start `line`, and takes the one at it.
    pub(super) fn quote_line(&mut self, line: usize) -> Option<Gutter> {
        let quote_lines = &mut self.quote_lines;
        self.out.extend(std::iter::from_fn(|| {
            quote_lines.next_if(|gutter| gutter.range.start.get() < line)
        }));
        self.quote_lines
            .next_if(|gutter| gutter.range.start.get() == line)
    }

    /// The lines from the line start `from` to `to` that an item holds, where no item opens. A
    /// line's item part hides its blanks before its leaf's text, where pulldown-cmark starts it: a
    /// fence at its first byte, a code or an HTML line its leftover columns before its first text
    /// event. Any other line hides its blanks below the innermost item's hang column, and takes no
    /// item part when blank (D11). Stops where no item holds the line.
    pub(super) fn held_lines(&mut self, items: &Items, from: usize, to: usize) {
        let text = self.text;
        let bytes = text.as_bytes();
        let mut next = from;
        // Holders are released and checked first: an empty hold searches no line end (D1).
        while next < to {
            let line = next;
            self.release(line);
            let Some(&Held { hang, .. }) = self.held.last() else {
                return;
            };
            let end = line_end(text, line);
            next = end + 1;
            let quoted = self.quote_line(line);
            let start = quoted
                .as_ref()
                .map_or(line, |gutter| gutter.range.end.get());
            let is_blank = |byte: u8| matches!(byte, b' ' | b'\t');
            let leaf = self
                .leaf_text(line, end)
                .map(|leaf| column_after(0, &bytes[line..leaf.start]) - leaf.leftover);
            let fence = leaf.is_none() && self.in_block(line, end);
            if leaf.is_none() && !fence && bytes[start..end].iter().all(|&byte| is_blank(byte)) {
                self.out.extend(quoted);
                continue;
            }
            let hides = |at: usize, after: usize| match leaf {
                Some(text) => after <= text,
                None => fence || at < hang,
            };
            let blanks = bytes[start..end]
                .iter()
                .scan(column_after(0, &bytes[line..start]), |column, &byte| {
                    let at = *column;
                    *column = column_after(at, &[byte]);
                    Some((at, *column, byte))
                })
                .take_while(|&(at, after, byte)| is_blank(byte) && hides(at, after))
                .count();
            let mut gutter = quoted.unwrap_or_else(|| gutter_at(line, 0));
            gutter.item =
                Some(items.item_part(ByteOffset::new(start), &self.held, &[], &mut self.keys));
            gutter.range.end = ByteOffset::new(start + blanks);
            self.out.push(gutter);
        }
    }

    /// The first text event of a code or an HTML block on the line from `line` to `end`; lines come
    /// in order.
    fn leaf_text(&mut self, line: usize, end: usize) -> Option<&LeafText> {
        while self.leaf_texts.next_if(|leaf| leaf.start < line).is_some() {}
        self.leaf_texts.next_if(|leaf| leaf.start <= end)
    }

    /// Whether a code block holds the line from `line` to `end`; lines come in order.
    fn in_block(&mut self, line: usize, end: usize) -> bool {
        while self.blocks.next_if(|block| block.end < line).is_some() {}
        self.blocks.peek().is_some_and(|block| block.start <= end)
    }
}
