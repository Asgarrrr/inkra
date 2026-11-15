//! What a change clears and what it lexes, from the blocks and the layer's chunks alone (code
//! highlighting spec, D7).

use std::ops::Range;

use inkra_text::{ByteOffset, Splice};

/// Block bytes one listener call lexes on the main thread; a call at this size takes at most 0.5 ms (D10).
pub const MAIN_THREAD_BUDGET: usize = 65_536;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Each held chunk that is no block's range, in order (spec deviation 6).
    pub clear: Vec<Range<ByteOffset>>,
    /// The index in `blocks` of each block to lex, in order (spec deviation 1).
    pub lex: Vec<usize>,
}

/// One merge of `blocks`, each with its reach start, and the `held` chunks; both sorted, disjoint
/// and non-empty.
#[must_use]
pub fn plan(
    blocks: &[(Range<ByteOffset>, ByteOffset)],
    held: &[Range<ByteOffset>],
    dirty: Option<Range<ByteOffset>>,
) -> Plan {
    // A dirty range that only touches a reach meets it.
    let dirty = dirty.map(|Range { start, end }| start..=end);
    let mut clear = Vec::new();
    let mut lex = Vec::new();
    let mut held = held.iter().peekable();
    for (index, (block, reach_start)) in blocks.iter().enumerate() {
        while let Some(chunk) = held.next_if(|chunk| chunk.start < block.start) {
            clear.push(chunk.clone());
        }
        let exact = held.next_if_eq(&block).is_some();
        let meets = dirty
            .as_ref()
            .is_some_and(|dirty| *dirty.start() <= block.end && reach_start <= dirty.end());
        if !exact || meets {
            lex.push(index);
        }
    }
    clear.extend(held.cloned());
    Plan { clear, lex }
}

/// The splices, in order, as one range in the text after the last; `None` for none.
#[must_use]
pub fn dirty(splices: impl IntoIterator<Item = Splice>) -> Option<Range<ByteOffset>> {
    splices.into_iter().fold(None, |acc, s| {
        Some(match acc {
            None => s.at..s.at + s.inserted_len,
            Some(r) => {
                r.start.min(s.at)..r.end.max(s.at + s.removed_len) - s.removed_len + s.inserted_len
            }
        })
    })
}

#[cfg(test)]
mod tests;
