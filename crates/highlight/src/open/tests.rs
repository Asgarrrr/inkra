use std::ops::Range;

use inkra_text::ByteOffset;

use super::*;
use crate::TokenClass;

fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

/// `let` is a keyword at 8..11, `mut` one at 23..26.
const TWO_BLOCKS: &str = "```rust\nlet\n```\n\n```rs\nmut x\n```\n";

fn ranges(chunks: &[BlockChunk]) -> Vec<Range<ByteOffset>> {
    chunks.iter().map(|(range, _, _)| range.clone()).collect()
}

/// A rust block, then prose up to `len` bytes.
fn padded(len: usize) -> String {
    let block = "```rust\nlet\n```\n";
    format!("{block}{}", "x".repeat(len - block.len()))
}

#[test]
fn open_colours_every_block_before_its_deadline() {
    let shapes: Vec<_> = parse(TWO_BLOCKS)
        .code_blocks
        .iter()
        .map(|block| block.shape)
        .collect();

    let chunks = open_chunks(TWO_BLOCKS, || true);

    assert_eq!(
        chunks,
        [
            (r(0, 15), shapes[0], vec![(r(8, 11), TokenClass::Keyword)]),
            (r(17, 32), shapes[1], vec![(r(23, 26), TokenClass::Keyword)]),
        ]
    );
}

#[test]
fn open_past_its_deadline_lands_only_the_seed() {
    let chunks = open_chunks(TWO_BLOCKS, || false);

    assert_eq!(chunks, []);
}

#[test]
fn a_text_over_the_parse_limit_lands_only_the_seed() {
    let chunks = open_chunks(&padded(OPEN_PARSE_LIMIT + 1), || true);

    assert_eq!(chunks, []);
}

#[test]
fn a_text_at_the_parse_limit_is_coloured() {
    let chunks = open_chunks(&padded(OPEN_PARSE_LIMIT), || true);

    assert_eq!(ranges(&chunks), [r(0, 15)]);
}

#[test]
fn open_stops_at_the_chunk_its_deadline_passes() {
    let mut calls = 0;
    let chunks = open_chunks(TWO_BLOCKS, || {
        calls += 1;
        calls == 1
    });

    assert_eq!(ranges(&chunks), [r(0, 15)]);
}
