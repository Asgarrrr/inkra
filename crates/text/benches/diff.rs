use std::hint::black_box;
use std::time::Instant;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use inkra_text::{Buffer, diff};

const WORDS: [&str; 20] = [
    "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "lorem", "ipsum", "dolor",
    "sit", "amet", "vault", "buffer", "editor", "undo", "anchor", "grapheme", "markdown",
];

/// Prose of about `bytes`: four-line paragraphs, a blank line between them; `seed` picks the words.
fn note(bytes: usize, seed: u64) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut state = seed;
    let mut line = 0;
    while text.len() < bytes {
        if line % 5 == 4 {
            text.push('\n');
        } else {
            for word in 0..8 + line % 5 {
                if word > 0 {
                    text.push(' ');
                }
                // A linear congruential step, so no line repeats with a short period.
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                text.push_str(WORDS[usize::try_from(state >> 33).unwrap_or(0) % WORDS.len()]);
            }
            text.push('\n');
        }
        line += 1;
    }
    text
}

fn one_line_changed(old: &str) -> String {
    let middle = old.len() / 2;
    let start = old[..middle].rfind('\n').map_or(0, |index| index + 1);
    let end = old[middle..]
        .find('\n')
        .map_or(old.len(), |index| middle + index);
    format!("{}a changed line{}", &old[..start], &old[end..])
}

fn bench_case(c: &mut Criterion, name: &str, old: &str, new: &str) {
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(old.len() as u64));
    group.bench_function("diff", |b| {
        b.iter(|| diff(0, black_box(old), black_box(new)));
    });
    let diff = diff(0, old, new);
    let now = Instant::now();
    group.bench_function("apply", |b| {
        b.iter_batched(
            || Buffer::new(old),
            |mut buffer| {
                assert!(buffer.apply_diff(now, &diff));
                buffer
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_diff(c: &mut Criterion) {
    let note_50k = note(50_000, 1);
    bench_case(
        c,
        "diff_50kb_one_line",
        &note_50k,
        &one_line_changed(&note_50k),
    );
    bench_case(
        c,
        "diff_1mb_rewritten",
        &note(1_000_000, 1),
        &note(1_000_000, 3),
    );
}

criterion_group!(benches, bench_diff);
criterion_main!(benches);
