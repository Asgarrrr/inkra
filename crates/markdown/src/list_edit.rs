//! The list commands' shared reading of the parse: the caret's line, a line's holder, and the
//! container prefix every inserted `\n` takes (list editing spec, D1, D2, D9).

use std::ops::Range;

use inkra_text::{ByteOffset, Hunk};

use crate::items::{Marker, column_after};
use crate::spans::{last_before, line_end, line_ranges, line_start, to_offsets};
use crate::{Gutter, Markdown};

pub use newline::continue_list;

/// A list command's edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEdit {
    /// In order and disjoint, as `Diff::new` asserts.
    pub hunks: Vec<Hunk>,
    /// Where the caret lands, as an offset in the text before the edit: the hunks move it as an
    /// anchor biased right, `Diff::map(caret, Bias::Right)`.
    pub caret: ByteOffset,
}

impl ListEdit {
    fn new(hunks: Vec<Hunk>, caret: usize) -> Self {
        Self {
            hunks,
            caret: ByteOffset::new(caret),
        }
    }

    /// One hunk replacing `range` with `text`, the caret at the end of its text.
    fn replace(range: Range<usize>, text: String) -> Self {
        Self::new(vec![hunk(range.clone(), text)], range.end)
    }
}

fn hunk(range: Range<usize>, text: String) -> Hunk {
    Hunk {
        range: to_offsets(range),
        text,
    }
}

/// A note's text and its parse, which every list command reads together.
#[derive(Clone, Copy)]
struct Doc<'a> {
    text: &'a str,
    markdown: &'a Markdown,
}

impl<'a> Doc<'a> {
    /// The line holding `offset`.
    fn line_at(self, offset: usize) -> Line<'a> {
        let start = line_start(self.text, offset);
        self.line(start..line_end(self.text, start))
    }

    fn line(self, range: Range<usize>) -> Line<'a> {
        Line {
            doc: self,
            gutter: self.markdown.gutter_on(ByteOffset::new(range.start)),
            range,
        }
    }

    /// The innermost gutter item holding the line starting at `line`: the last item whose marker
    /// lies before it, walked up through its parents until a gutter item ends past it (D1).
    fn holder(self, line: usize) -> Option<usize> {
        let items = &self.markdown.structure.items;
        let mut at = last_before(items, line, |item| item.marker.get())?;
        loop {
            if items[at].end.get() > line && self.is_gutter_item(at) {
                return Some(at);
            }
            at = items[at].parent()?;
        }
    }

    /// Whether item `at` is a gutter item: its marker lies in its marker line's gutter, so in its
    /// item part, since no marker lies in a quote part.
    fn is_gutter_item(self, at: usize) -> bool {
        let marker = self.markdown.structure.items[at].marker;
        let line = ByteOffset::new(line_start(self.text, marker.get()));
        self.markdown
            .gutter_on(line)
            .is_some_and(|gutter| marker < gutter.range.end)
    }

    /// The column where item `at`'s content starts on its marker line (`CommonMark` 5.2).
    fn content_column(self, at: usize) -> usize {
        let marker = self.markdown.structure.items[at].marker.get();
        Marker::at(self.text, line_start(self.text, marker), marker).content_column
    }

    /// What follows a `\n` that starts a line in the quotes of the line at `line`: its `>`s, then
    /// spaces to `column` (D2).
    fn container_prefix(self, line: usize, column: usize) -> String {
        let Self { text, markdown } = self;
        let mut prefix = String::new();
        let depth = markdown
            .gutter_on(ByteOffset::new(line))
            .map_or(0, |gutter| gutter.depth);
        let at = markdown
            .gutters
            .partition_point(|gutter| gutter.range.start.get() <= line);
        // A lazy line shows fewer `>`; the quote's lines above it hold one that shows them all.
        let source = markdown.gutters[..at]
            .iter()
            .rev()
            .find(|gutter| gutter.quotes.len() >= depth);
        if let Some(source) = source {
            let mut from = source.range.start.get();
            for quote in &source.quotes[..depth] {
                prefix.push_str(&text[from..quote.get()]);
                prefix.push_str("> ");
                from = quote.get() + 1;
                from += usize::from(text.as_bytes().get(from).is_some_and(|&b| is_blank(b)));
            }
        }
        let written = column_after(0, prefix.as_bytes());
        prefix.extend(std::iter::repeat_n(' ', column.saturating_sub(written)));
        prefix
    }
}

/// A line as the parse reads it: its range without its `\n`, and its gutter.
struct Line<'a> {
    doc: Doc<'a>,
    range: Range<usize>,
    gutter: Option<&'a Gutter>,
}

impl<'a> Line<'a> {
    /// The lines below it.
    fn below(&self) -> impl Iterator<Item = Line<'a>> + use<'a> {
        let doc = self.doc;
        line_ranges(doc.text, self.range.start)
            .skip(1)
            .map(move |range| doc.line(range))
    }

    /// The first byte past its quote part and item part.
    fn text_start(&self) -> usize {
        self.gutter
            .map_or(self.range.start, |gutter| gutter.range.end.get())
    }

    /// The end of its quote part, past its last `>` and the blank after it.
    fn quote_end(&self) -> usize {
        self.gutter
            .map_or(self.range.start, |gutter| gutter.quote_end().get())
    }

    /// The end of its quote part without the blank after its last `>`, which a line left blank
    /// keeps (D2).
    fn trimmed_quote_end(&self) -> usize {
        let end = self.quote_end();
        let blank = end > self.range.start && is_blank(self.doc.text.as_bytes()[end - 1]);
        end - usize::from(blank)
    }

    /// The end of its leading blanks past its quote part.
    fn lead_end(&self) -> usize {
        let from = self.quote_end();
        from + blanks(&self.doc.text[from..self.range.end])
    }

    /// The column of `at`, a byte on the line, counted from its start.
    fn column(&self, at: usize) -> usize {
        column_after(0, &self.doc.text.as_bytes()[self.range.start..at])
    }

    /// Whether it has only blanks past its quote part.
    fn is_blank(&self) -> bool {
        self.is_blank_past(self.quote_end())
    }

    /// Whether it has only blanks past `at`, a byte on it.
    fn is_blank_past(&self, at: usize) -> bool {
        blanks(&self.doc.text[at..self.range.end]) == self.range.end - at
    }

    /// The end of its text before `caret`, without the blanks before `caret`; its text start when
    /// it has none.
    fn typed_end(&self, caret: usize) -> usize {
        let start = self.text_start();
        start
            + self.doc.text[start..caret]
                .trim_end_matches([' ', '\t'])
                .len()
    }

    /// Whether it continues the paragraph above only by laziness: no item opens on it, and it
    /// shows fewer `>` than it is in, or starts left of its holder's content column.
    fn is_lazy(&self) -> bool {
        let gutter = self.gutter;
        let marked = gutter
            .and_then(|gutter| gutter.item.as_ref())
            .is_some_and(|item| item.marked);
        !marked
            && !self.is_blank()
            && (gutter.is_some_and(|gutter| gutter.quotes.len() < gutter.depth)
                || self
                    .doc
                    .holder(self.range.start)
                    .is_some_and(|at| self.column(self.lead_end()) < self.doc.content_column(at)))
    }

    /// Its container prefix to `column` in place of its leading blanks, when it is lazy, which a
    /// blank line above would end (D2, rules 2, 6 and 8).
    fn lazy_prefix(&self, column: usize) -> Option<Hunk> {
        self.is_lazy().then(|| {
            let prefix = self.doc.container_prefix(self.range.start, column);
            hunk(self.range.start..self.lead_end(), prefix)
        })
    }
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// The length of `text`'s leading blanks.
fn blanks(text: &str) -> usize {
    text.bytes().take_while(|&byte| is_blank(byte)).count()
}

mod newline;
mod shift;

#[cfg(test)]
mod tests;
