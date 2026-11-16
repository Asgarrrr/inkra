//! What a change clears and what it lexes, from the blocks and the layer's chunks alone (code
//! highlighting spec, D7).

use std::ops::Range;

use inkra_text::ByteOffset;

/// The block bytes a listener call lexes within 0.5 ms (D10); `batch` lexes no more, but the app
/// lexes the backlog in the same call before S2c.
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
/// block's shape (deviation 10).
#[must_use]
pub(crate) fn plan(
    blocks: &[(Range<ByteOffset>, u64)],
    held: &[(Range<ByteOffset>, Option<u64>)],
) -> Plan {
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
        if !exact {
            lex.push(index);
        }
    }
    clear.extend(held.map(|(chunk, _)| chunk.clone()));
    Plan { clear, lex }
}

#[cfg(test)]
mod tests;
