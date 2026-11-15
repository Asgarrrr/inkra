use std::hint::black_box;
use std::time::Instant;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use inkra_editor::Layer;
use inkra_text::{Buffer, ByteOffset};

/// The highlighting spec's code-heavy note: 51 blocks, one chunk each.
const BLOCKS: usize = 51;

const BLOCK: &str = "```rust\nfn block() -> usize {\n    let x = 1;\n    x + 1\n}\n```\n\n";

/// The note, and a layer with one chunk per block and a mark on each `fn` and `let`.
fn layered() -> (Buffer, Layer<u8>) {
    let buffer = Buffer::new(BLOCK.repeat(BLOCKS));
    let mut layer = Layer::new(buffer.edit_mark());
    for block in 0..BLOCKS {
        let start = block * BLOCK.len();
        let marks = ["fn", "let"]
            .into_iter()
            .map(|word| {
                let at = start + BLOCK.find(word).unwrap_or(0);
                (ByteOffset::new(at)..ByteOffset::new(at + word.len()), 1)
            })
            .collect();
        let chunk = ByteOffset::new(start)..ByteOffset::new(start + BLOCK.len());
        let published = layer.publish(&buffer, chunk, 0, marks, buffer.edit_mark());
        assert_eq!(published, Ok(true));
    }
    (buffer, layer)
}

fn bench_catch_up(c: &mut Criterion) {
    let now = Instant::now();
    let middle = ByteOffset::new(BLOCKS / 2 * BLOCK.len() + BLOCK.find("x + 1").unwrap_or(0));
    c.bench_function("layer/catch_up_one_splice_51_chunks", |b| {
        b.iter_batched(
            || {
                let (mut buffer, layer) = layered();
                buffer.transact_at(now, |buffer| buffer.insert(middle, "y"));
                (buffer, layer)
            },
            |(buffer, mut layer)| {
                assert_eq!(layer.catch_up(black_box(&buffer)), Ok(()));
                layer
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_catch_up);
criterion_main!(benches);
