//! Baseline timings for `Buffer`'s size-dependent operations.
//!
//! `Buffer` stores the whole document as one `String` plus a derived line
//! index (`buffer.rs`), so every operation here either rescans the text or
//! shifts bytes in it — the costs soft wrap and virtualization will build on
//! top of (`docs/roadmap.md`, Soft wrap and Virtualization sections). Not part
//! of `check.sh`, and no baseline is committed: run before and after a change
//! that touches a hot path and compare the two reports by eye.
//!
//! `transact_at` takes an `Instant` the buffer never reads on its own; each
//! group reads the clock once, outside every timed closure, and reuses it.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_text::{Buffer, ByteOffset};

/// Line counts spanning the range a real vault note can reach: `1_000` is an
/// ordinary note, `100_000` is the size a soft-wrap or virtualization profile
/// would be run against.
const SIZES: [usize; 3] = [1_000, 10_000, 100_000];

/// How far from the end of the document the "near the end" benchmarks probe —
/// close enough to exercise a near-full scan without landing exactly on the
/// last byte.
const NEAR_END_MARGIN: usize = 200;

const WORDS: [&str; 20] = [
    "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "lorem", "ipsum", "dolor",
    "sit", "amet", "vault", "buffer", "editor", "undo", "anchor", "grapheme", "markdown",
];

/// One line per call, cycling through a handful of non-ASCII scripts so the
/// UTF-16 and grapheme paths — which only do real work on multi-byte text —
/// are exercised somewhere in the document.
const MULTIBYTE_LINES: [&str; 3] = [
    "Résumé café naïve façade coöperate.",
    "Emoji check: 🎉🚀✨ combined with a family cluster 👨‍👩‍👧.",
    "CJK sample: 日本語のテキストです。中文测试文本。",
];

/// A markdown-ish document of `line_count` lines: headings every fifth line,
/// a non-ASCII line every thirty-seventh, ordinary word-salad otherwise.
fn build_document(line_count: usize) -> String {
    let mut text = String::with_capacity(line_count * 32);
    for line in 0..line_count {
        if line % 37 == 0 {
            text.push_str(MULTIBYTE_LINES[(line / 37) % MULTIBYTE_LINES.len()]);
        } else if line % 5 == 0 {
            let _ = write!(text, "## Heading {line}");
        } else {
            let word_count = 6 + (line % 5);
            for word_index in 0..word_count {
                if word_index > 0 {
                    text.push(' ');
                }
                text.push_str(WORDS[(line + word_index) % WORDS.len()]);
            }
        }
        text.push('\n');
    }
    text
}

/// The byte offset `document.len() / 2`, snapped onto a char boundary.
fn middle_offset(document: &str) -> ByteOffset {
    Buffer::new(document.to_string()).clip(ByteOffset(document.len() / 2))
}

/// A byte offset `NEAR_END_MARGIN` bytes before the end, snapped onto a char
/// boundary.
fn near_end_offset(document: &str) -> ByteOffset {
    let target = document.len().saturating_sub(NEAR_END_MARGIN);
    Buffer::new(document.to_string()).clip(ByteOffset(target))
}

/// `Buffer::new` moves the `String` in rather than copying it, so this
/// measures construction itself, not the clone `iter_batched`'s setup pays
/// for on every sample.
fn bench_new(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_new");
    for lines in SIZES {
        let document = build_document(lines);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(lines),
            &document,
            |b, document| {
                b.iter_batched(|| document.clone(), Buffer::new, BatchSize::LargeInput);
            },
        );
    }
    group.finish();
}

/// A single-char insert at the start, middle, and end of the document — the
/// three positions `replace`'s byte-shift cost differs most between.
fn bench_insert(c: &mut Criterion) {
    let now = Instant::now();
    let mut group = c.benchmark_group("buffer_insert_char");
    for lines in SIZES {
        let document = build_document(lines);
        let end = ByteOffset(document.len());
        let positions = [
            ("start", ByteOffset(0)),
            ("middle", middle_offset(&document)),
            ("end", end),
        ];
        group.throughput(Throughput::Bytes(document.len() as u64));
        for (position, offset) in positions {
            group.bench_with_input(
                BenchmarkId::new(position, lines),
                &document,
                |b, document| {
                    b.iter_batched(
                        || Buffer::new(document.clone()),
                        |mut buffer| {
                            buffer.transact_at(now, |buffer| buffer.insert(offset, "x"));
                            buffer
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }
    group.finish();
}

/// Reverting the transaction a single insert opened.
fn bench_undo(c: &mut Criterion) {
    let now = Instant::now();
    let mut group = c.benchmark_group("buffer_undo");
    for lines in SIZES {
        let document = build_document(lines);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(lines),
            &document,
            |b, document| {
                b.iter_batched(
                    || {
                        let mut buffer = Buffer::new(document.clone());
                        let end = buffer.len();
                        buffer.transact_at(now, |buffer| buffer.insert(end, "x"));
                        buffer
                    },
                    |mut buffer| {
                        buffer.undo();
                        buffer
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_line_for_offset(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_line_for_offset");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let middle = middle_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.line_for_offset(black_box(middle)));
        });
    }
    group.finish();
}

fn bench_line_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_line_range");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let middle_line = buffer.line_count() / 2;
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.line_range(black_box(middle_line)));
        });
    }
    group.finish();
}

fn bench_to_utf16(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_to_utf16");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let near_end = near_end_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.to_utf16(black_box(near_end)));
        });
    }
    group.finish();
}

fn bench_from_utf16(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_from_utf16");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let near_end = buffer.to_utf16(near_end_offset(&document));
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.from_utf16(black_box(near_end)));
        });
    }
    group.finish();
}

fn bench_next_grapheme(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_next_grapheme");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let middle = middle_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.next_grapheme(black_box(middle)));
        });
    }
    group.finish();
}

fn bench_previous_grapheme(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_previous_grapheme");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let middle = middle_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.previous_grapheme(black_box(middle)));
        });
    }
    group.finish();
}

fn bench_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_lines_iteration");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| {
                buffer
                    .lines()
                    .map(|(at, line)| at.0 + line.len())
                    .sum::<usize>()
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_new,
    bench_insert,
    bench_undo,
    bench_line_for_offset,
    bench_line_range,
    bench_to_utf16,
    bench_from_utf16,
    bench_next_grapheme,
    bench_previous_grapheme,
    bench_lines,
);
criterion_main!(benches);
