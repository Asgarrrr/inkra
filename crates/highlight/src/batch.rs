//! One listener call's clears and lexed blocks, from the parse and the layer's chunks (code
//! highlighting spec, D7 and deviation 9).

use std::ops::Range;

use inkra_markdown::{CodeBlock, Markdown, Span, content_lines};
use inkra_text::ByteOffset;

use crate::plan::{Plan, plan};
use crate::{TokenClass, block_tokens};

/// The longest block, in bytes, fences included, that `batch` lexes; a longer one gets an empty chunk
/// (D10, deviation 11).
pub(crate) const BLOCK_LIMIT: usize = 524_288;

/// A code block's range, its shape and its tokens: one block chunk of the syntax layer.
pub type BlockChunk = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, TokenClass)>);

#[derive(Debug, PartialEq, Eq)]
pub struct Batch {
    /// Each held chunk that is no block's range, in order (deviation 6).
    pub clear: Vec<Range<ByteOffset>>,
    /// The chunk of each block `plan` lexes, in order; empty for an unknown language or a block over
    /// `BLOCK_LIMIT` (deviation 11).
    pub lexed: Vec<BlockChunk>,
}

/// What one listener call publishes over `text`, parsed as `markdown`, given the layer's `held`
/// chunks (D7); the clears go first in the publish.
#[must_use]
pub fn batch(text: &str, markdown: &Markdown, held: &[(Range<ByteOffset>, Option<u64>)]) -> Batch {
    let blocks: Vec<_> = markdown
        .code_blocks
        .iter()
        .map(|block| (block.range.clone(), block.shape))
        .collect();
    let Plan { clear, lex } = plan(&blocks, held);
    let lexed = lex
        .into_iter()
        .map(|index| {
            let block = &markdown.code_blocks[index];
            if block.range.end - block.range.start > BLOCK_LIMIT {
                (block.range.clone(), block.shape, Vec::new())
            } else {
                block_chunk(text, &markdown.spans, block)
            }
        })
        .collect();
    Batch { clear, lexed }
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
