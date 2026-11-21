//! The width shift: the blanks a batch adds to an item's other lines when its content column
//! moves, so they stay in the item (list editing spec, D7).

use inkra_text::Hunk;

use super::{Line, hunk};

/// `by` spaces at the end of `line`'s leading blanks past its quote part: there, no tab after them
/// takes them into its own columns.
pub(super) fn shifted(line: &Line<'_>, text: &str, by: usize) -> Hunk {
    let at = line.lead_end(text);
    hunk(at..at, " ".repeat(by))
}
