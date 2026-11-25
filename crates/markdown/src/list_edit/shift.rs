//! The width shift: the blanks a batch adds to, or cuts from, an item's other lines when its
//! content column moves, so they stay in the item (list editing spec, D4, D7).

use std::num::NonZeroUsize;

use inkra_text::Hunk;

use super::{Doc, Line, hunk};
use crate::items::marker_end;
use crate::spans::line_end;

/// `by` spaces at the end of `line`'s leading blanks past its quote part: there, no tab after them
/// takes them into its own columns.
pub(super) fn shifted(line: &Line<'_>, by: NonZeroUsize) -> Hunk {
    let at = line.lead_end();
    hunk(at..at, " ".repeat(by.get()))
}

/// `by` columns cut from the end of `line`'s leading blanks past its quote part, or all of them
/// when it has fewer, as a lazy line can. A cut inside a tab rewrites the blanks as spaces first,
/// from the last `>`, whose tab splits its columns between the quote and the item (D4).
fn cut(line: &Line<'_>, by: usize) -> Option<Hunk> {
    let (from, end) = (line.quote_end(), line.lead_end());
    let target = line.column(end).saturating_sub(by);
    let base = line.trimmed_quote_end();
    if target >= line.column(from) {
        if let Some(at) = (from..=end).find(|&at| line.column(at) == target) {
            return (at < end).then(|| hunk(at..end, String::new()));
        }
    } else if line.doc.text.as_bytes()[base..from] != *b"\t" {
        return (from < end).then(|| hunk(from..end, String::new()));
    }
    let target = target.max(line.column(base));
    Some(hunk(base..end, " ".repeat(target - line.column(base))))
}

/// The columns each line moves by and the markers a command rewrites, written as hunks in order.
/// A line that several items move takes their sum, as one hunk (D7).
#[derive(Default)]
pub(super) struct Moves {
    /// Each moved line's start and the columns it moves by, in any order.
    lines: Vec<(usize, isize)>,
    markers: Vec<Hunk>,
}

impl Moves {
    pub(super) fn line(&mut self, start: usize, by: isize) {
        self.lines.push((start, by));
    }

    pub(super) fn lines<'a>(&mut self, lines: impl Iterator<Item = Line<'a>>, by: isize) {
        if by != 0 {
            self.lines.extend(lines.map(|line| (line.range.start, by)));
        }
    }

    /// The marker at `at` rewritten as `marker`, when it differs.
    pub(super) fn marker(&mut self, text: &str, at: usize, marker: String) {
        let end = marker_end(text, at);
        if text[at..end] != marker {
            self.markers.push(hunk(at..end, marker));
        }
    }

    /// The hunks, in order and disjoint: a blank line takes none.
    pub(super) fn hunks(mut self, doc: Doc<'_>) -> Vec<Hunk> {
        self.lines.sort_unstable_by_key(|&(start, _)| start);
        let mut hunks = self.markers;
        let mut lines = self.lines.into_iter().peekable();
        while let Some((start, mut by)) = lines.next() {
            while let Some((_, more)) = lines.next_if(|&(next, _)| next == start) {
                by += more;
            }
            let line = doc.line(start..line_end(doc.text, start));
            if line.is_blank() {
                continue;
            }
            let moved = match NonZeroUsize::new(by.unsigned_abs()) {
                Some(columns) if by > 0 => Some(shifted(&line, columns)),
                Some(columns) => cut(&line, columns.get()),
                None => None,
            };
            hunks.extend(moved);
        }
        // An insertion before a marker sorts before the marker's rewrite.
        hunks.sort_unstable_by_key(|hunk| (hunk.range.start, hunk.range.end));
        hunks
    }
}
