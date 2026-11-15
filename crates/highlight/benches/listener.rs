//! One listener call's pure part, lex and publish together, by size; it sets `MAIN_THREAD_BUDGET`
//! (code highlighting spec, D10 and deviation 5).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_highlight::{Batch, MAIN_THREAD_BUDGET, TokenClass, batch};
use inkra_markdown::{Markdown, parse};
use inkra_text::Buffer;

const TEXT: &str = include_str!("fixtures/rust-5000.txt");
const BLOCK_BYTES: usize = 512;
const BLOCKS: usize = 1000;

/// The shortest prefix of whole lines at least `bytes` long.
fn code(bytes: usize) -> &'static str {
    let end = TEXT[bytes..].find('\n').map_or(TEXT.len(), |at| bytes + at);
    &TEXT[..end]
}

/// The listener's pure part: catch up, read the chunks, then publish the batch.
fn call(buffer: &Buffer, layer: &mut Layer<TokenClass>, markdown: &Markdown) -> usize {
    layer.catch_up(buffer).expect("the log holds the edits");
    let held: Vec<_> = layer.chunks().collect();
    let Batch { clear, lexed } = batch(buffer.text(), markdown, &held);
    let count = lexed.len();
    layer
        .publish(buffer, clear, lexed, buffer.edit_mark())
        .expect("the batch lands");
    count
}

fn sizes() -> Vec<usize> {
    let mut sizes = vec![
        4 << 10,
        8 << 10,
        16 << 10,
        32 << 10,
        64 << 10,
        128 << 10,
        MAIN_THREAD_BUDGET,
    ];
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

fn bench_one_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("listener_one_block");
    for bytes in sizes() {
        let buffer = Buffer::new(format!("```rust\n{}\n```\n", code(bytes)));
        let markdown = parse(buffer.text());
        let fresh = || Layer::new(buffer.edit_mark());
        assert_eq!(call(&buffer, &mut fresh(), &markdown), 1);
        group.bench_function(BenchmarkId::from_parameter(bytes), |b| {
            b.iter_batched(
                fresh,
                |mut layer| {
                    call(&buffer, &mut layer, &markdown);
                    layer
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_many_blocks(c: &mut Criterion) {
    let block = format!("```rust\n{}\n```\n\n", code(BLOCK_BYTES));
    let buffer = Buffer::new(block.repeat(BLOCKS));
    let markdown = parse(buffer.text());
    let Batch { lexed, .. } = batch(buffer.text(), &markdown, &[]);
    assert_eq!(lexed.len(), BLOCKS);
    // The first `n` blocks hold their own marks under another tag, as after a renamed info word:
    // a call lexes them, and each publish compares equal marks in full (deviation 10).
    let held = |n: usize| {
        let mut layer = Layer::new(buffer.edit_mark());
        let (stale, exact) = lexed.split_at(n);
        let chunks = stale
            .iter()
            .map(|(range, shape, marks)| (range.clone(), shape ^ 1, marks.clone()))
            .chain(exact.iter().cloned())
            .collect();
        layer
            .publish(&buffer, Vec::new(), chunks, buffer.edit_mark())
            .expect("the blocks land");
        layer
    };
    let mut group = c.benchmark_group("listener_many_blocks");
    for bytes in sizes() {
        let n = bytes.div_ceil(code(BLOCK_BYTES).len());
        assert_eq!(call(&buffer, &mut held(n), &markdown), n);
        group.bench_function(BenchmarkId::from_parameter(bytes), |b| {
            b.iter_batched(
                || held(n),
                |mut layer| {
                    call(&buffer, &mut layer, &markdown);
                    layer
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_one_block, bench_many_blocks);
criterion_main!(benches);
