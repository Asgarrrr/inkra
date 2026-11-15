//! Which block chunks the open function lands (code highlighting spec, D8).

use std::iter;

use inkra_markdown::{Markdown, parse};

use crate::BlockChunk;
use crate::batch::block_chunk;

/// The longest text, in bytes, the open function parses; a longer one lands no chunk (D8).
pub const OPEN_PARSE_LIMIT: usize = 500_000;

/// The block chunks `text` lands at open: none over `OPEN_PARSE_LIMIT`, where it is not parsed;
/// else each block's, in document order, while `before_deadline` holds before it is lexed.
#[must_use]
pub fn open_chunks(text: &str, before_deadline: impl FnMut() -> bool) -> Vec<BlockChunk> {
    if text.len() > OPEN_PARSE_LIMIT {
        return Vec::new();
    }
    let Markdown {
        spans, code_blocks, ..
    } = parse(text);
    let chunks = code_blocks
        .iter()
        .map(|block| block_chunk(text, &spans, block));
    before(chunks, before_deadline)
}

/// Each item of `items`, in order, while `before_deadline` holds before it is made.
fn before<T>(
    mut items: impl Iterator<Item = T>,
    mut before_deadline: impl FnMut() -> bool,
) -> Vec<T> {
    iter::from_fn(|| before_deadline().then(|| items.next()).flatten()).collect()
}

#[cfg(test)]
mod tests;
