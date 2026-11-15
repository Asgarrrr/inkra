use std::ops::Range;

use inkra_text::ByteOffset;

use super::*;
use crate::TokenClass;
use crate::fixtures::{TWO_BLOCKS, r};

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

/// An unknown-language block, a rust one, then another unknown-language one.
const NOPE_RUST_NOPE: &str = "```nope\nx\n```\n\n```rust\nlet\n```\n\n```nope\ny\n```\n";

#[test]
fn past_its_deadline_open_lands_the_empty_chunks_of_unknown_language_blocks() {
    let shapes: Vec<_> = parse(NOPE_RUST_NOPE)
        .code_blocks
        .iter()
        .map(|block| block.shape)
        .collect();

    let chunks = open_chunks(NOPE_RUST_NOPE, || false);

    assert_eq!(
        chunks,
        [
            (r(0, 13), shapes[0], vec![]),
            (r(32, 45), shapes[2], vec![])
        ]
    );
}

#[test]
fn an_unknown_language_block_spends_no_deadline_check() {
    let text = format!("{NOPE_RUST_NOPE}\n```rust\nmut\n```\n");
    let mut calls = 0;

    let chunks = open_chunks(&text, || {
        calls += 1;
        calls == 1
    });

    assert_eq!(ranges(&chunks), [r(0, 13), r(15, 30), r(32, 45)]);
}

#[test]
fn open_asks_no_deadline_once_it_passed() {
    let text = format!("{TWO_BLOCKS}\n```rust\nfn\n```\n");
    let mut calls = 0;

    let chunks = open_chunks(&text, || {
        calls += 1;
        assert!(calls <= 2, "a deadline asked once it passed");
        calls == 1
    });

    assert_eq!(ranges(&chunks), [r(0, 15)]);
}
