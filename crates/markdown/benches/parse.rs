use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_markdown::{cycle_tasks, parse};
use inkra_text::ByteOffset;

/// Bytes: the live-preview spec's X1 table, then the sizes that set `OPEN_PARSE_LIMIT`.
const SIZES: [usize; 6] = [5_000, 50_000, 500_000, 1_000_000, 2_000_000, 4_000_000];

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

/// `⌘A ⌘L`: the batch over every line, from the cached parse (task boxes spec, D6).
fn bench_cycle_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("cycle_tasks_all");
    for bytes in [50_000, 500_000] {
        let note = build_note(bytes);
        let markdown = parse(&note);
        let all = ByteOffset::new(0)..ByteOffset::new(note.len());
        group.throughput(Throughput::Bytes(note.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &note, |b, note| {
            b.iter(|| cycle_tasks(black_box(note), &markdown, all.clone()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse, bench_cycle_all);
criterion_main!(benches);
