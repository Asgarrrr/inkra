//! One listener call's clears, lexed blocks and pending blocks, from the parse and the layer's
//! chunks (code highlighting spec, D7, D10 and deviation 9).

use std::ops::Range;

use inkra_markdown::{CodeBlock, Markdown, Span, content_lines};
use inkra_text::ByteOffset;

use crate::plan::{MAIN_THREAD_BUDGET, Plan, plan};
use crate::{TokenClass, block_tokens, info_language};

/// The longest block, in bytes, fences included, that `batch` lexes; a longer one gets an empty chunk
/// (D10, deviation 11).
pub const BLOCK_LIMIT: usize = 524_288;

/// A code block's range, its shape and its tokens: one block chunk of the syntax layer.
pub type BlockChunk = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, TokenClass)>);

#[derive(Debug, PartialEq, Eq)]
pub struct Batch {
    /// Each held chunk that is no block's range, in order (deviation 6).
    pub clear: Vec<Range<ByteOffset>>,
    /// In order, the empty chunk of each block `plan` lexes with an unknown language or over
    /// `BLOCK_LIMIT` (deviation 11), and the chunk of each the main thread lexes (D10).
    pub lexed: Vec<BlockChunk>,
    /// The index of each other block `plan` lexes, in order: the backlog's (D10).
    pub pending: Vec<usize>,
}

/// What one listener call publishes over `text`, parsed as `markdown`, given the layer's `held`
/// chunks, and what it leaves pending (D7, D10); the clears go first in the publish. The main thread
/// lexes the blocks `dirty` meets, touching included, while they fit in `MAIN_THREAD_BUDGET`.
#[must_use]
pub fn batch(
    text: &str,
    markdown: &Markdown,
    held: &[(Range<ByteOffset>, Option<u64>)],
    dirty: Option<Range<ByteOffset>>,
) -> Batch {
    let blocks: Vec<_> = markdown
        .code_blocks
        .iter()
        .map(|block| (block.range.clone(), block.shape))
        .collect();
    let Plan { clear, lex } = plan(&blocks, held);
    let meets = move |block: &Range<ByteOffset>| {
        dirty
            .as_ref()
            .is_some_and(|dirty| dirty.start <= block.end && block.start <= dirty.end)
    };
    let (mut lexed, mut pending) = (Vec::new(), Vec::new());
    let mut budget = MAIN_THREAD_BUDGET;
    for index in lex {
        let block = &markdown.code_blocks[index];
        let len = block.range.end - block.range.start;
        if len > BLOCK_LIMIT || info_language(text, block.language.clone()).is_none() {
            lexed.push((block.range.clone(), block.shape, Vec::new()));
        } else if len <= budget && meets(&block.range) {
            budget -= len;
            lexed.push(block_chunk(text, &markdown.spans, block));
        } else {
            pending.push(index);
        }
    }
    Batch {
        clear,
        lexed,
        pending,
    }
}

/// `block`'s tokens over its content lines; none when its info word names no known language.
#[must_use]
pub fn block_marks(
    text: &str,
    spans: &[Span],
    block: &CodeBlock,
) -> Vec<(Range<ByteOffset>, TokenClass)> {
    block_tokens(
        text,
        block.language.clone(),
        content_lines(text, spans, block),
    )
}

pub(crate) fn block_chunk(text: &str, spans: &[Span], block: &CodeBlock) -> BlockChunk {
    let marks = block_marks(text, spans, block);
    (block.range.clone(), block.shape, marks)
}

#[cfg(test)]
mod tests;
