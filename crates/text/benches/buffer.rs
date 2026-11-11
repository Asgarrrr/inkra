use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_text::{Bias, Buffer, ByteOffset};

const SIZES: [usize; 3] = [1_000, 10_000, 100_000];

const NEAR_END_MARGIN: usize = 200;

const WORDS: [&str; 20] = [
    "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "lorem", "ipsum", "dolor",
    "sit", "amet", "vault", "buffer", "editor", "undo", "anchor", "grapheme", "markdown",
];

const MULTIBYTE_LINES: [&str; 3] = [
    "Résumé café naïve façade coöperate.",
    "Emoji check: 🎉🚀✨ combined with a family cluster 👨‍👩‍👧.",
    "CJK sample: 日本語のテキストです。中文测试文本。",
];

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

fn middle_offset(document: &str) -> ByteOffset {
    Buffer::new(document.to_string()).clip(ByteOffset::new(document.len() / 2))
}

fn near_end_offset(document: &str) -> ByteOffset {
    let target = document.len().saturating_sub(NEAR_END_MARGIN);
    Buffer::new(document.to_string()).clip(ByteOffset::new(target))
}

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

fn bench_insert(c: &mut Criterion) {
    let now = Instant::now();
    let mut group = c.benchmark_group("buffer_insert_char");
    for lines in SIZES {
        let document = build_document(lines);
        let end = ByteOffset::new(document.len());
        let positions = [
            ("start", ByteOffset::new(0)),
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

/// A tab switch parks three anchors and later forgets them; `Buffer` keeps every slot.
fn bench_insert_after_parks(c: &mut Criterion) {
    let now = Instant::now();
    let mut group = c.benchmark_group("buffer_insert_after_parks");
    let mut document = String::new();
    let mut lines = 0;
    while document.len() < 10_000 {
        lines += 1;
        document = build_document(lines);
    }
    let near_end = near_end_offset(&document);
    group.throughput(Throughput::Bytes(document.len() as u64));
    for parks in [0, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(parks),
            &document,
            |b, document| {
                b.iter_batched(
                    || {
                        let mut buffer = Buffer::new(document.clone());
                        for _ in 0..parks {
                            let anchors = [Bias::Right, Bias::Left, Bias::Left]
                                .map(|bias| buffer.anchor(near_end, bias));
                            for anchor in anchors {
                                buffer.forget(anchor);
                            }
                        }
                        buffer
                    },
                    |mut buffer| {
                        buffer.transact_at(now, |buffer| buffer.insert(near_end, "x"));
                        buffer
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

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

fn bench_line_at(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_line_at");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let middle = middle_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| buffer.line_at(black_box(middle)));
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

/// The IME's `selected_text_range` and its inverse, repeated, after a select-all.
fn bench_utf16_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_utf16_range");
    for lines in SIZES {
        let document = build_document(lines);
        let buffer = Buffer::new(document.clone());
        let all = ByteOffset::new(0)..ByteOffset::new(document.len());
        let utf16 = buffer.to_utf16_range(&all);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &buffer, |b, buffer| {
            b.iter(|| {
                black_box(buffer.to_utf16_range(black_box(&all)));
                black_box(buffer.from_utf16_range(black_box(&utf16)));
            });
        });
    }
    group.finish();
}

/// The IME's round trip at the caret just after a keystroke.
fn bench_utf16_after_insert(c: &mut Criterion) {
    let now = Instant::now();
    let mut group = c.benchmark_group("buffer_utf16_after_insert");
    for lines in SIZES {
        let document = build_document(lines);
        let near_end = near_end_offset(&document);
        group.throughput(Throughput::Bytes(document.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(lines),
            &document,
            |b, document| {
                b.iter_batched(
                    || {
                        let buffer = Buffer::new(document.clone());
                        let _ = buffer.to_utf16(near_end);
                        buffer
                    },
                    |mut buffer| {
                        buffer.transact_at(now, |buffer| buffer.insert(near_end, "x"));
                        let utf16 = black_box(buffer.to_utf16(near_end + 1));
                        black_box(buffer.from_utf16(utf16));
                        buffer
                    },
                    BatchSize::LargeInput,
                );
            },
        );
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
                    .map(|(at, line)| at.get() + line.len())
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
    bench_insert_after_parks,
    bench_undo,
    bench_line_for_offset,
    bench_line_range,
    bench_line_at,
    bench_to_utf16,
    bench_from_utf16,
    bench_utf16_range,
    bench_utf16_after_insert,
    bench_next_grapheme,
    bench_previous_grapheme,
    bench_lines,
);
criterion_main!(benches);
