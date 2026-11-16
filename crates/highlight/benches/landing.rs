//! One landing's publish: a backlog's chunks at an older edit mark, with 200 splices since,
//! scattered over the note; it sets `TASK_BYTES` and checks `BLOCK_LIMIT` (code highlighting spec,
//! D10).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_highlight::{BLOCK_LIMIT, BlockChunk, TASK_BYTES, TokenClass, block_marks};
use inkra_markdown::parse;
use inkra_text::{Buffer, ByteOffset, EditMark};

/// What an `apply_diff` reload during the flight leaves; the splice log keeps 256.
const SPLICES: usize = 200;
const BLOCK_BYTES: usize = 512;
/// About one mark per byte: each char is its own punctuation token.
const DENSE: &str = "(),;";

/// A rust block of dense code whose range is about `len` bytes, fences included.
fn dense_block(len: usize) -> String {
    let line = DENSE.repeat(16);
    let lines = len
        .saturating_sub("```rust\n```".len())
        .div_ceil(line.len() + 1);
    format!(
        "```rust\n{}```\n\n",
        format!("{line}\n").repeat(lines.max(1))
    )
}

/// A note of `bytes` block bytes in each of the three worse cases.
fn notes(bytes: usize) -> [(&'static str, String); 3] {
    [
        ("one_block", dense_block(bytes)),
        (
            "dense_blocks",
            dense_block(BLOCK_BYTES).repeat(bytes / BLOCK_BYTES),
        ),
        ("empty_blocks", "```rust\n```\n\n".repeat(bytes / 11)),
    ]
}

/// Every block of `buffer`'s text, lexed: the chunks a backlog of them lands.
fn chunks(buffer: &Buffer) -> Vec<BlockChunk> {
    let text = buffer.text();
    let markdown = parse(text);
    markdown
        .code_blocks
        .iter()
        .map(|block| {
            let marks = block_marks(text, &markdown.spans, block);
            (block.range.clone(), block.shape, marks)
        })
        .collect()
}

/// `SPLICES` one-byte replacements, evenly spaced over the text.
fn scatter(buffer: &mut Buffer) {
    let step = buffer.text().len() / (SPLICES + 1);
    for k in 1..=SPLICES {
        let at = ByteOffset::new(k * step);
        buffer.transact(|t| t.replace(at..at + 1, "x"));
    }
}

/// The layer as the landing finds it: the same blocks held under another tag, caught up to `buffer`.
fn held(buffer: &Buffer, chunks: &[BlockChunk], at: EditMark) -> Layer<TokenClass> {
    let mut layer = Layer::new(at);
    let stale = chunks
        .iter()
        .map(|(range, shape, marks)| (range.clone(), shape ^ 1, marks.clone()))
        .collect();
    layer
        .publish(buffer, Vec::new(), stale, at)
        .expect("the blocks land");
    layer
}

fn bench_note(c: &mut Criterion, group: &str, id: BenchmarkId, note: &str) {
    let mut buffer = Buffer::new(note);
    let at = buffer.edit_mark();
    let chunks = chunks(&buffer);
    scatter(&mut buffer);
    c.benchmark_group(group).bench_function(id, |b| {
        b.iter_batched(
            || (held(&buffer, &chunks, at), chunks.clone()),
            |(mut layer, chunks)| {
                layer
                    .publish(&buffer, Vec::new(), chunks, at)
                    .expect("the landing lands");
                layer
            },
            BatchSize::LargeInput,
        );
    });
}

fn bench_landing(c: &mut Criterion) {
    let mut sizes = vec![
        4 << 10,
        8 << 10,
        16 << 10,
        32 << 10,
        64 << 10,
        128 << 10,
        TASK_BYTES,
    ];
    sizes.sort_unstable();
    sizes.dedup();
    for bytes in sizes {
        for (case, note) in notes(bytes) {
            let id = BenchmarkId::new(case, bytes);
            bench_note(c, "landing", id, &note);
        }
    }
}

fn bench_block_limit(c: &mut Criterion) {
    let id = BenchmarkId::from_parameter(BLOCK_LIMIT);
    bench_note(c, "landing_block_limit", id, &dense_block(BLOCK_LIMIT));
}

criterion_group!(benches, bench_landing, bench_block_limit);
criterion_main!(benches);
