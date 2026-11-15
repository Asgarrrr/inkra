//! What a change clears and what it lexes, from the blocks and the layer's chunks alone (code
//! highlighting spec, D7).

use std::ops::Range;

use inkra_text::{ByteOffset, Splice};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Each held chunk with marks that is no block's range, in order.
    pub clear: Vec<Range<ByteOffset>>,
    /// The index in `blocks` of each block to lex, in order (spec deviation 1).
    pub lex: Vec<usize>,
}

/// One merge of `blocks`, each with its reach start, and the `held` chunks, each with whether it
/// holds a mark; both sorted, disjoint and non-empty.
#[must_use]
pub fn plan(
    blocks: &[(Range<ByteOffset>, ByteOffset)],
    held: &[(Range<ByteOffset>, bool)],
    dirty: Option<Range<ByteOffset>>,
) -> Plan {
    // A dirty range that only touches a reach meets it.
    let dirty = dirty.map(|Range { start, end }| start..=end);
    let mut clear = Vec::new();
    let mut lex = Vec::new();
    let mut held = held.iter().peekable();
    let mut clear_marked = |(chunk, marked): &(Range<ByteOffset>, bool)| {
        if *marked {
            clear.push(chunk.clone());
        }
    };
    for (index, (block, reach_start)) in blocks.iter().enumerate() {
        while let Some(chunk) = held.next_if(|(chunk, _)| chunk.start < block.start) {
            clear_marked(chunk);
        }
        let exact = held.next_if(|(chunk, _)| chunk == block).is_some();
        let meets = dirty
            .as_ref()
            .is_some_and(|dirty| *dirty.start() <= block.end && reach_start <= dirty.end());
        if !exact || meets {
            lex.push(index);
        }
    }
    held.for_each(clear_marked);
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
