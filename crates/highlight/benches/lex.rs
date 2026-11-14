use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_highlight::{Language, highlight};
use inkra_text::ByteOffset;

const TEXTS: [(usize, &str); 3] = [
    (20, include_str!("fixtures/rust-20.txt")),
    (500, include_str!("fixtures/rust-500.txt")),
    (5000, include_str!("fixtures/rust-5000.txt")),
];

fn lines(text: &str) -> Vec<(ByteOffset, &str)> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let at = ByteOffset::new(start);
            start += line.len() + 1;
            (at, line)
        })
        .collect()
}

fn bench_lex(c: &mut Criterion) {
    let mut group = c.benchmark_group("lex");
    for (n, text) in TEXTS {
        let lines = lines(text);
        let mut out = Vec::new();
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(n), |b| {
            b.iter(|| {
                out.clear();
                highlight(Language::Rust, lines.iter().copied(), black_box(&mut out));
                black_box(&out);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_lex);
criterion_main!(benches);
