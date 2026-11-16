//! One listener call's pure part, lex and publish together, by size; it sets `MAIN_THREAD_BUDGET`
//! (code highlighting spec, D10 and deviation 5). Past the budget the call lexes its backlogs too,
//! as the app's does before the background task; the backlog's copy is measured on its own.

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_highlight::{
    Batch, MAIN_THREAD_BUDGET, TASK_BYTES, TokenClass, backlog, batch, lex_backlog,
};
use inkra_markdown::{Markdown, parse};
use inkra_text::{Buffer, ByteOffset};

const TEXT: &str = include_str!("fixtures/rust-5000.txt");
const BLOCK_BYTES: usize = 512;
const BLOCKS: usize = 1000;

/// The shortest prefix of whole lines at least `bytes` long.
fn code(bytes: usize) -> &'static str {
    let end = TEXT[bytes..].find('\n').map_or(TEXT.len(), |at| bytes + at);
    &TEXT[..end]
}

/// The listener's pure part on a change over the whole text: catch up, read the chunks, lex the
/// backlogs, then publish the batch.
fn call(buffer: &Buffer, layer: &mut Layer<TokenClass>, markdown: &Markdown) -> usize {
    layer.catch_up(buffer).expect("the log holds the edits");
    let held: Vec<_> = layer.chunks().collect();
    let text = buffer.text();
    let dirty = Some(ByteOffset::new(0)..buffer.len());
    let Batch {
        clear,
        mut lexed,
        pending,
    } = batch(text, markdown, &held, dirty);
    let mut rest = pending.as_slice();
    while !rest.is_empty() {
        let landed = lex_backlog(backlog(text, markdown, rest));
        rest = &rest[landed.len()..];
        lexed.extend(landed);
    }
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
    let Batch { pending, .. } = batch(buffer.text(), &markdown, &[], None);
    assert_eq!(pending.len(), BLOCKS);
    let lexed = lex_backlog(backlog(buffer.text(), &markdown, &pending));
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

/// `backlog` alone at `TASK_BYTES`: one block, and blocks of `BLOCK_BYTES`.
fn bench_backlog_copy(c: &mut Criterion) {
    let one = format!("```rust\n{}\n```\n", code(TASK_BYTES));
    let block = format!("```rust\n{}\n```\n\n", code(BLOCK_BYTES));
    let many = block.repeat(TASK_BYTES.div_ceil(block.len() - 2));
    let mut group = c.benchmark_group("listener_backlog_copy");
    for (case, text) in [("one_block", one), ("many_blocks", many)] {
        let markdown = parse(&text);
        let Batch { pending, .. } = batch(&text, &markdown, &[], None);
        group.bench_function(BenchmarkId::new(case, TASK_BYTES), |b| {
            b.iter(|| backlog(&text, &markdown, &pending));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_one_block,
    bench_many_blocks,
    bench_backlog_copy
);
criterion_main!(benches);
