//! The width shift: the blanks a batch adds to an item's other lines when its content column
//! moves, so they stay in the item (list editing spec, D7).

use std::num::NonZeroUsize;

use inkra_text::Hunk;

use super::{Line, hunk};

/// `by` spaces at the end of `line`'s leading blanks past its quote part: there, no tab after them
/// takes them into its own columns.
pub(super) fn shifted(line: &Line<'_>, by: NonZeroUsize) -> Hunk {
    let at = line.lead_end();
    hunk(at..at, " ".repeat(by.get()))
}
