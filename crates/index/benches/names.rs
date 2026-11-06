#[path = "names/paths.rs"]
mod paths;

use std::collections::HashSet;
use std::hint::black_box;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{NoteContent, NoteIndex, NoteSet};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Mtime, Stamp, Vault};
use paths::{Lcg, folders};

/// The stamp of every text the bench parses: nothing here compares stamps.
const EPOCH: Stamp = Stamp {
    mtime: Mtime(UNIX_EPOCH),
    size: 0,
};

const NOTES: usize = 50_000;
/// Built once: a `MemFs` scan of this vault takes minutes.
static ENTRIES: LazyLock<Vec<(EntryId, String, Kind, Mtime)>> = LazyLock::new(entries);

fn entries() -> Vec<(EntryId, String, Kind, Mtime)> {
    let fs = MemFs::new();
    let mut lcg = Lcg(7);
    for note in 0..NOTES {
        let depth = 3 + lcg.next(3);
        fs.insert_file(format!("/vault/{} {note}.md", folders(&mut lcg, depth)), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new("/vault"))) else {
        panic!("a MemFs scan cannot fail");
    };
    vault
        .entries()
        .map(|entry| {
            (
                entry.id,
                entry.path.as_path().to_string_lossy().into_owned(),
                entry.kind,
                entry.mtime,
            )
        })
        .collect()
}

fn bench_build(c: &mut Criterion) {
    c.bench_function("NoteSet::build/50k notes", |b| {
        b.iter_batched(|| ENTRIES.clone(), NoteSet::build, BatchSize::LargeInput);
    });
}

fn bench_set_notes(c: &mut Criterion) {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    for (id, _, kind, _) in ENTRIES.iter() {
        if *kind == Kind::Note {
            index.insert(*id, NoteContent::parse("".into(), EPOCH));
        }
    }
    let mut group = c.benchmark_group("set_notes");
    // Each iteration's setup is a 50k build, so fewer samples keep the run short.
    group.sample_size(10);
    group.bench_function("50k with a built NoteSet", |b| {
        b.iter_batched(
            || NoteSet::build(ENTRIES.clone()),
            |notes| index.set_notes(notes),
            BatchSize::LargeInput,
        );
    });
    bench_prune(&mut group);
    bench_set_notes_with_text(&mut group);
    group.finish();
}

/// Every note links `[[note n+1]]` and `[[index]]`, so the `index` stem row holds 50k links.
fn bench_prune(group: &mut BenchmarkGroup<'_, WallTime>) {
    let notes: Vec<EntryId> = ENTRIES
        .iter()
        .filter(|(_, _, kind, _)| *kind == Kind::Note)
        .map(|(id, ..)| *id)
        .collect();
    let texts: Vec<Arc<str>> = (0..notes.len())
        .map(|n| format!("[[note {}]] [[index]]", n + 1).into())
        .collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    for (id, text) in notes.iter().zip(&texts) {
        index.insert(*id, NoteContent::parse(Arc::clone(text), EPOCH));
    }

    let mut other = Some(NoteSet::build(ENTRIES.clone()));
    group.bench_function("nothing gone", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let Some(set) = other.take() else {
                    unreachable!("the set is put back every iteration");
                };
                let start = Instant::now();
                let previous = black_box(index.set_notes(set));
                total += start.elapsed();
                other = Some(previous);
            }
            total
        });
    });

    for (case, count) in [
        ("the first 500 notes gone", 500),
        ("the first 5,500 notes gone", 5_500),
    ] {
        let gone: HashSet<EntryId> = notes[..count].iter().copied().collect();
        let kept = ENTRIES
            .iter()
            .filter(|(id, ..)| !gone.contains(id))
            .cloned()
            .collect();
        let mut partial = Some(NoteSet::build(kept));
        group.bench_function(case, |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let Some(set) = partial.take() else {
                        unreachable!("the set is put back every iteration");
                    };
                    let start = Instant::now();
                    let full = black_box(index.set_notes(set));
                    total += start.elapsed();
                    partial = Some(index.set_notes(full));
                    for (id, text) in notes[..count].iter().zip(&texts) {
                        index.insert(*id, NoteContent::parse(Arc::clone(text), EPOCH));
                    }
                }
                total
            });
        });
    }
}

/// Every note holds a text, so `set_notes` moves 50k slots; with a snapshot held, it first
/// copies the pointers.
fn bench_set_notes_with_text(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    for (n, (id, _, kind, _)) in ENTRIES.iter().enumerate() {
        if *kind == Kind::Note {
            let text = format!("note {n}\n");
            index.insert(*id, NoteContent::parse(text.into(), EPOCH));
        }
    }
    let mut other = Some(NoteSet::build(ENTRIES.clone()));
    for (case, held) in [
        ("50k, text in every slot", false),
        ("50k, text in every slot, a snapshot held", true),
    ] {
        group.bench_function(case, |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let Some(set) = other.take() else {
                        unreachable!("the set is put back every iteration");
                    };
                    let snapshot = held.then(|| index.corpus());
                    let start = Instant::now();
                    let previous = black_box(index.set_notes(set));
                    total += start.elapsed();
                    drop(snapshot);
                    other = Some(previous);
                }
                total
            });
        });
    }
}

criterion_group!(benches, bench_build, bench_set_notes);
criterion_main!(benches);
