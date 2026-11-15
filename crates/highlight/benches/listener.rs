//! One listener call's pure part, lex and publish together, by size; it sets `MAIN_THREAD_BUDGET`
//! (code highlighting spec, D10 and deviation 5).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use std::ops::Range;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_highlight::{Language, MAIN_THREAD_BUDGET, Plan, TokenClass, highlight, plan};
use inkra_markdown::{CodeBlock, Markdown, Span, content_lines, parse};
use inkra_text::{Buffer, ByteOffset};

const TEXT: &str = include_str!("fixtures/rust-5000.txt");
const BLOCK_BYTES: usize = 512;
const BLOCKS: usize = 1000;

/// The shortest prefix of whole lines at least `bytes` long.
fn code(bytes: usize) -> &'static str {
    let end = TEXT[bytes..].find('\n').map_or(TEXT.len(), |at| bytes + at);
    &TEXT[..end]
}

fn tokens(text: &str, spans: &[Span], block: &CodeBlock) -> Vec<(Range<ByteOffset>, TokenClass)> {
    let mut tokens = Vec::new();
    let language = block
        .language
        .clone()
        .map(|word| &text[word.start.get()..word.end.get()])
        .and_then(Language::from_info);
    if let Some(language) = language {
        highlight(language, content_lines(text, spans, block), &mut tokens);
    }
    tokens
}

/// The listener's pure part: catch up, read the chunks, plan, then one batch of clears and lexed
/// blocks.
fn call(
    buffer: &Buffer,
    layer: &mut Layer<TokenClass>,
    markdown: &Markdown,
    dirty: Option<Range<ByteOffset>>,
) -> usize {
    layer.catch_up(buffer).expect("the log holds the edits");
    let held: Vec<_> = layer.chunks().collect();
    let blocks: Vec<_> = markdown
        .code_blocks
        .iter()
        .map(|b| (b.range.clone(), b.shape))
        .collect();
    let Plan { clear, lex } = plan(&blocks, &held, dirty);
    let lexed: Vec<_> = lex
        .iter()
        .map(|&index| {
            let block = &markdown.code_blocks[index];
            let marks = tokens(buffer.text(), &markdown.spans, block);
            (block.range.clone(), block.shape, marks)
        })
        .collect();
    layer
        .publish(buffer, clear, lexed, buffer.edit_mark())
        .expect("the batch lands");
    lex.len()
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
        let whole = ByteOffset::new(0)..buffer.len();
        let fresh = || Layer::new(buffer.edit_mark());
        assert_eq!(
            call(&buffer, &mut fresh(), &markdown, Some(whole.clone())),
            1
        );
        group.bench_function(BenchmarkId::from_parameter(bytes), |b| {
            b.iter_batched(
                fresh,
                |mut layer| {
                    call(&buffer, &mut layer, &markdown, Some(whole.clone()));
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
    let mut layer = Layer::new(buffer.edit_mark());
    let whole = ByteOffset::new(0)..buffer.len();
    assert_eq!(call(&buffer, &mut layer, &markdown, Some(whole)), BLOCKS);
    let mut group = c.benchmark_group("listener_many_blocks");
    for bytes in sizes() {
        let n = bytes.div_ceil(code(BLOCK_BYTES).len());
        let dirty = markdown.code_blocks[0].range.start..markdown.code_blocks[n - 1].range.end;
        assert_eq!(call(&buffer, &mut layer, &markdown, Some(dirty.clone())), n);
        group.bench_function(BenchmarkId::from_parameter(bytes), |b| {
            b.iter_batched(
                || dirty.clone(),
                |dirty| call(&buffer, &mut layer, &markdown, Some(dirty)),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_one_block, bench_many_blocks);
criterion_main!(benches);
