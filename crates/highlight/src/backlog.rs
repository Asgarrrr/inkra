//! The pending blocks' code, copied out of the text so another thread can lex it (code highlighting
//! spec, D10).

use std::ops::Range;

use inkra_markdown::{Markdown, content_lines};
use inkra_text::ByteOffset;

use crate::{BlockChunk, Language, highlight, info_language};

/// The block bytes, fences included, one backlog holds past its first block (D10).
pub const TASK_BYTES: usize = 32_768;

/// Pending blocks' content lines, owned, each block with its range, shape and language.
pub struct Backlog {
    blocks: Vec<Block>,
    /// Each content line's start in the text and its span in `code`.
    lines: Vec<(ByteOffset, Range<usize>)>,
    code: String,
}

struct Block {
    range: Range<ByteOffset>,
    shape: u64,
    language: Option<Language>,
    /// Its content lines' span in `Backlog::lines`.
    lines: Range<usize>,
}

/// The `pending` blocks of `text`, parsed as `markdown`, in order, up to the one that would take the
/// backlog past `TASK_BYTES` block bytes; the first always goes.
#[must_use]
pub fn backlog(text: &str, markdown: &Markdown, pending: &[usize]) -> Backlog {
    let mut backlog = Backlog {
        blocks: Vec::new(),
        lines: Vec::new(),
        code: String::new(),
    };
    let mut bytes = 0;
    for &index in pending {
        let block = &markdown.code_blocks[index];
        let len = block.range.end - block.range.start;
        if !backlog.blocks.is_empty() && bytes + len > TASK_BYTES {
            break;
        }
        bytes += len;
        let first = backlog.lines.len();
        for (start, line) in content_lines(text, &markdown.spans, block) {
            let at = backlog.code.len();
            backlog.code.push_str(line);
            backlog.lines.push((start, at..backlog.code.len()));
        }
        backlog.blocks.push(Block {
            range: block.range.clone(),
            shape: block.shape,
            language: info_language(text, block.language.clone()),
            lines: first..backlog.lines.len(),
        });
    }
    backlog
}

/// Each block of `backlog`, in order, with its tokens: its block chunk.
#[must_use]
pub fn lex_backlog(backlog: Backlog) -> Vec<BlockChunk> {
    let Backlog {
        blocks,
        lines,
        code,
    } = backlog;
    blocks
        .into_iter()
        .map(|block| {
            let mut tokens = Vec::new();
            if let Some(language) = block.language {
                let lines = lines[block.lines].iter();
                let lines = lines.map(|(start, at)| (*start, &code[at.clone()]));
                highlight(language, lines, &mut tokens);
            }
            (block.range, block.shape, tokens)
        })
        .collect()
}

/// The `pending` blocks' chunks, in order, backlog after backlog, on the caller's thread: what the
/// app's background task lands over its flights, drained at once for the benches.
#[must_use]
pub fn lex_pending(text: &str, markdown: &Markdown, mut pending: &[usize]) -> Vec<BlockChunk> {
    let mut lexed = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let landed = lex_backlog(backlog(text, markdown, pending));
        pending = &pending[landed.len()..];
        lexed.extend(landed);
    }
    lexed
}

#[cfg(test)]
mod tests;
