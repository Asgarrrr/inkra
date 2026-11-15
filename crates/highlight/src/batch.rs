//! One listener call's clears and lexed blocks, from the parse and the layer's chunks (code
//! highlighting spec, D7 and deviation 9).

use std::ops::Range;

use inkra_markdown::{CodeBlock, Markdown, Span, content_lines};
use inkra_text::ByteOffset;

use crate::plan::{Plan, plan};
use crate::{TokenClass, block_tokens};

/// A code block's range, its shape and its tokens: one block chunk of the syntax layer.
pub type BlockChunk = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, TokenClass)>);

#[derive(Debug, PartialEq, Eq)]
pub struct Batch {
    /// Each held chunk that is no block's range, in order (deviation 6).
    pub clear: Vec<Range<ByteOffset>>,
    /// Each block `plan` lexes, in order.
    pub lexed: Vec<BlockChunk>,
}

/// What one listener call publishes over `text`, parsed as `markdown`, given the layer's `held`
/// chunks and the `dirty` range (D7); the clears go first in the publish.
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
    let Plan { clear, lex } = plan(&blocks, held, dirty);
    let lexed = lex
        .into_iter()
        .map(|index| block_chunk(text, &markdown.spans, &markdown.code_blocks[index]))
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
