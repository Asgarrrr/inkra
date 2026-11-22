//! The width shift: the blanks a batch adds to an item's other lines when its content column
//! moves, so they stay in the item (list editing spec, D7).

use inkra_text::Hunk;

use super::{Line, hunk};

/// `by` spaces at the end of `line`'s leading blanks past its quote part: there, no tab after them
/// takes them into its own columns. No spaces, no hunk.
pub(super) fn shifted(line: &Line<'_>, by: usize) -> Option<Hunk> {
    let at = line.lead_end();
    (by > 0).then(|| hunk(at..at, " ".repeat(by)))
}
