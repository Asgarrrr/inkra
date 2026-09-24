use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_markdown::parse;

/// Bytes, matching the live-preview spec's X1 table.
const SIZES: [usize; 3] = [5_000, 50_000, 500_000];

const SECTION: &str = "## A section heading\n\
\n\
Plain prose with **strong**, *emphasis*, ~~struck~~ and `code`, then a\n\
[link](https://example.com/path), a [[Wiki Page|wikilink]] and a [[Bare Link]].\n\
A second line of the same paragraph, long enough to wrap once at seventy columns.\n\
\n\
> A quote with *emphasis* that runs onto\n\
> a second line.\n\
\n\
- a bullet with **bold**\n\
- [ ] a task\n\
  - a nested bullet\n\
1. a numbered item\n\
\n\
```rust\n\
let x = 1;\n\
```\n\
\n";

fn build_note(bytes: usize) -> String {
    let mut text = String::from("---\ntitle: Bench\n---\n# Title\n\n");
    while text.len() < bytes {
        text.push_str(SECTION);
    }
    text
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_parse");
    for bytes in SIZES {
        let note = build_note(bytes);
        group.throughput(Throughput::Bytes(note.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &note, |b, note| {
            b.iter(|| parse(black_box(note)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
