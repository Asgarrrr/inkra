//! What a change clears and what it lexes, from the blocks and the layer's chunks alone (code
//! highlighting spec, D7).

use std::ops::Range;

use inkra_text::{ByteOffset, Splice};

/// The block bytes a listener call lexes within 0.5 ms (D10); until S2, no call is capped at it.
pub const MAIN_THREAD_BUDGET: usize = 65_536;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Each held chunk that is no block's range, in order (spec deviation 6).
    pub(crate) clear: Vec<Range<ByteOffset>>,
    /// The index in `blocks` of each block to lex, in order (spec deviation 1).
    pub(crate) lex: Vec<usize>,
}

/// One merge of `blocks`, each with its shape, and the `held` chunks, each with its tag; both sorted,
/// disjoint and non-empty. A chunk on a block's range is kept, and lexed over unless its tag is the
/// block's shape and the dirty range misses the block.
#[must_use]
pub(crate) fn plan(
    blocks: &[(Range<ByteOffset>, u64)],
    held: &[(Range<ByteOffset>, Option<u64>)],
    dirty: Option<Range<ByteOffset>>,
) -> Plan {
    // A dirty range that only touches a block meets it: an empty one is a deletion there.
    let dirty = dirty.map(|Range { start, end }| start..=end);
    let mut clear = Vec::new();
    let mut lex = Vec::new();
    let mut held = held.iter().peekable();
    for (index, (block, shape)) in blocks.iter().enumerate() {
        while let Some((chunk, _)) = held.next_if(|(chunk, _)| chunk.start < block.start) {
            clear.push(chunk.clone());
        }
        let exact = held
            .next_if(|(chunk, _)| chunk == block)
            .is_some_and(|&(_, tag)| tag == Some(*shape));
        let meets = dirty
            .as_ref()
            .is_some_and(|dirty| *dirty.start() <= block.end && block.start <= *dirty.end());
        if !exact || meets {
            lex.push(index);
        }
    }
    clear.extend(held.map(|(chunk, _)| chunk.clone()));
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
