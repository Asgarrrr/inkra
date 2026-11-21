//! One landing's publish: a backlog's chunks at an older edit mark, with 200 splices since,
//! scattered over the note or in a prose paragraph above its blocks, or with none; it sets
//! `TASK_BYTES` and `BLOCK_LIMIT` (code highlighting spec, D10, S2b deviation 10).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_highlight::{BLOCK_LIMIT, BlockChunk, TASK_BYTES, TokenClass, block_marks};
use inkra_markdown::parse;
use inkra_text::{Buffer, ByteOffset};

/// What an `apply_diff` reload during the flight leaves; the splice log keeps 256.
const SPLICES: usize = 200;
const BLOCK_BYTES: usize = 512;
/// About one mark per byte: each char is its own punctuation token.
const DENSE: &str = "(),;";
/// The bytes of the prose paragraph `Splices::Above` edits, its blank line included.
const PROSE: usize = 4096;

/// Where the splices since the landing's edit mark lie.
#[derive(Clone, Copy)]
enum Splices {
    /// Spread over the note, so they reach most blocks, which then land nothing (S2b deviation 1).
    Scattered,
    /// In a prose paragraph above the blocks, so they reach none.
    Above,
    None,
}

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

/// `SPLICES` one-byte replacements, evenly spaced over the first `len` bytes of the text.
fn scatter(buffer: &mut Buffer, len: usize) {
    let step = len / (SPLICES + 1);
    for k in 1..=SPLICES {
        let at = ByteOffset::new(k * step);
        buffer.transact(|t| t.replace(at..at + 1, "x"));
    }
}

/// The layer as the landing finds it: the same blocks held under another tag at `base`, the note
/// before the scatter, then caught up to `buffer`.
fn held(base: &Buffer, buffer: &Buffer, chunks: &[BlockChunk]) -> Layer<TokenClass> {
    let mut layer = Layer::new(base.edit_mark());
    let stale = chunks
        .iter()
        .map(|(range, shape, marks)| (range.clone(), shape ^ 1, marks.clone()))
        .collect();
    layer
        .publish(base, Vec::new(), stale, base.edit_mark())
        .expect("the blocks land");
    layer.catch_up(buffer).expect("the log holds the scatter");
    layer
}

fn bench_note(c: &mut Criterion, group: &str, id: BenchmarkId, note: &str, splices: Splices) {
    let note = match splices {
        Splices::Above => format!("{}\n\n{note}", "p".repeat(PROSE - 2)),
        Splices::Scattered | Splices::None => note.to_owned(),
    };
    let (base, mut buffer) = (Buffer::new(&note), Buffer::new(&note));
    let at = buffer.edit_mark();
    // A late publish would skip every block a splice reached, so `held` publishes on a twin.
    assert_eq!(base.edit_mark(), at);
    let chunks = chunks(&buffer);
    match splices {
        Splices::Scattered => scatter(&mut buffer, note.len()),
        Splices::Above => scatter(&mut buffer, PROSE - 2),
        Splices::None => {}
    }
    c.benchmark_group(group).bench_function(id, |b| {
        b.iter_batched(
            || (held(&base, &buffer, &chunks), chunks.clone()),
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

/// `sizes` with `also`, sorted and without repeats.
fn with(mut sizes: Vec<usize>, also: usize) -> Vec<usize> {
    sizes.push(also);
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

fn bench_landing(c: &mut Criterion) {
    let sizes = (2..=7).map(|shift| 1 << (10 + shift)).collect();
    for bytes in with(sizes, TASK_BYTES) {
        for (case, note) in notes(bytes) {
            for (group, splices) in [
                ("landing", Splices::Scattered),
                ("landing_above", Splices::Above),
            ] {
                bench_note(c, group, BenchmarkId::new(case, bytes), &note, splices);
            }
        }
    }
}

/// One block up to twice the bound, with no splice reaching it: a reached block lands nothing.
fn bench_block_limit(c: &mut Criterion) {
    let sizes = (5..=9).map(|shift| 1 << (10 + shift)).collect();
    for bytes in with(sizes, BLOCK_LIMIT) {
        let note = dense_block(bytes);
        for (case, splices) in [("above", Splices::Above), ("none", Splices::None)] {
            let id = BenchmarkId::new(case, bytes);
            bench_note(c, "landing_block_limit", id, &note, splices);
        }
    }
}

criterion_group!(benches, bench_landing, bench_block_limit);
criterion_main!(benches);
