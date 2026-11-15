//! Which block chunks the open function lands (code highlighting spec, D8).

use inkra_markdown::{Markdown, parse};

use crate::batch::block_chunk;
use crate::{BlockChunk, info_language};

/// The longest text, in bytes, the open function parses; a longer one lands no chunk (D8).
pub const OPEN_PARSE_LIMIT: usize = 500_000;

/// The block chunks `text` lands at open: none over `OPEN_PARSE_LIMIT`, where it is not parsed;
/// else, in document order, each unknown-language block's empty chunk, and each other block's while
/// `before_deadline` holds before it is lexed (deviation 11).
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
        .filter(|block| {
            info_language(text, block.language.clone()).is_none() || {
                on_time = on_time && before_deadline();
                on_time
            }
        })
        .map(|block| block_chunk(text, &spans, block))
        .collect()
}

#[cfg(test)]
mod tests;
