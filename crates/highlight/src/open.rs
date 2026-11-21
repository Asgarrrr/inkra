//! Which block chunks the open function lands (code highlighting spec, D8).

use inkra_markdown::{Markdown, parse};

use crate::batch::{block_chunk, unlexed};
use crate::{BlockChunk, MAIN_THREAD_BUDGET};

/// The longest text, in bytes, the open function parses; a longer one lands no chunk (D8).
pub const OPEN_PARSE_LIMIT: usize = 500_000;

/// The block chunks `text` lands at open: none over `OPEN_PARSE_LIMIT`, where it is not parsed;
/// else, in document order, the empty chunk of each block with an unknown language or over
/// `BLOCK_LIMIT`, and each other block's while `before_deadline` holds before it is lexed
/// (deviation 11). A block over `MAIN_THREAD_BUDGET` gets none: the background task lexes it.
#[must_use]
pub fn open_chunks(text: &str, mut before_deadline: impl FnMut() -> bool) -> Vec<BlockChunk> {
    if text.len() > OPEN_PARSE_LIMIT {
        return Vec::new();
    }
    let Markdown {
        spans, code_blocks, ..
    } = parse(text);
    let mut on_time = true;
    code_blocks
        .iter()
        .filter_map(|block| {
            if unlexed(text, block) {
                return Some((block.range.clone(), block.shape, Vec::new()));
            }
            if block.range.end - block.range.start > MAIN_THREAD_BUDGET {
                return None;
            }
            on_time = on_time && before_deadline();
            on_time.then(|| block_chunk(text, &spans, block))
        })
        .collect()
}

#[cfg(test)]
mod tests;
