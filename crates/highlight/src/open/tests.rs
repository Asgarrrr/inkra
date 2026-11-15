use std::ops::Range;

use inkra_text::ByteOffset;

use super::*;
use crate::TokenClass;
use crate::fixtures::{TWO_BLOCKS, r};

fn made_past_the_deadline(_: char) -> char {
    panic!("an item made past the deadline")
}

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
fn no_item_is_made_past_the_deadline() {
    let items = before(['a', 'b'].into_iter().map(made_past_the_deadline), || false);

    assert_eq!(items, []);
}

#[test]
fn before_stops_at_the_item_its_deadline_passes() {
    let mut calls = 0;
    let items = before(
        ['a', 'b'].into_iter().map(|item| {
            if item == 'b' {
                made_past_the_deadline(item)
            } else {
                item
            }
        }),
        || {
            calls += 1;
            calls == 1
        },
    );

    assert_eq!(items, ['a']);
}
