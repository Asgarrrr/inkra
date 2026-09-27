#[path = "names/paths.rs"]
mod paths;

use std::collections::HashSet;
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{
    NameMatch, NameQuery, NamesSnapshot, NoteContent, NoteIndex, NoteSet, match_names, merge_names,
};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Mtime, Vault};
use paths::{Lcg, WORDS, folders, long_stem};

const NOTES: usize = 50_000;
const LIMIT: usize = 100;
/// Built once: a `MemFs` scan of this vault takes minutes.
static ENTRIES: LazyLock<Vec<(EntryId, String, Kind, Mtime)>> = LazyLock::new(entries);

fn entries() -> Vec<(EntryId, String, Kind, Mtime)> {
    let fs = MemFs::new();
    let mut lcg = Lcg(7);
    for note in 0..NOTES {
        let depth = 3 + lcg.next(3);
        let parts: Vec<&str> = (0..depth).map(|_| WORDS[lcg.next(WORDS.len())]).collect();
        fs.insert_file(format!("/vault/{} {note}.md", parts.join("/")), "");
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

/// Generated notes without a scan: matching reads only the paths, and a `MemFs` scan of
/// these trees would take far longer than the bench.
fn generated(path: impl Fn(&mut Lcg, usize) -> String) -> Vec<(EntryId, String, Kind, Mtime)> {
    let mut lcg = Lcg(11);
    (0..NOTES)
        .map(|note| {
            (
                EntryId::DETACHED,
                path(&mut lcg, note),
                Kind::Note,
                Mtime(UNIX_EPOCH),
            )
        })
        .collect()
}

fn long_stems() -> Vec<(EntryId, String, Kind, Mtime)> {
    generated(long_stem)
}

/// 7 to 9 components: 6 to 8 folders, then a stem `<word> <n>`.
fn deep() -> Vec<(EntryId, String, Kind, Mtime)> {
    generated(|lcg, note| {
        let depth = 7 + lcg.next(3);
        format!("{} {note}.md", folders(lcg, depth))
    })
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
            index.insert(*id, NoteContent::parse("".into(), Mtime(UNIX_EPOCH)));
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
        index.insert(*id, NoteContent::parse(Arc::clone(text), Mtime(UNIX_EPOCH)));
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
                        index.insert(*id, NoteContent::parse(Arc::clone(text), Mtime(UNIX_EPOCH)));
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
            index.insert(*id, NoteContent::parse(text.into(), Mtime(UNIX_EPOCH)));
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

fn bench_cases(c: &mut Criterion, vault: &str, names: &NamesSnapshot, cases: &[(&str, String)]) {
    let cancel = AtomicBool::new(false);
    for (case, text) in cases {
        let query = NameQuery::new(text, None, &[]);
        c.bench_function(&format!("match_names/50k {vault}, {case}"), |b| {
            b.iter(|| black_box(match_names(names, 0..names.len(), &query, &cancel, LIMIT)));
        });
    }
}

fn bench_match(c: &mut Criterion) {
    let names = NoteSet::build(ENTRIES.clone()).names();
    let distinct: String = "aeinrstlou"
        .chars()
        .flat_map(|a| "aeinrstlou".chars().map(move |b| format!("{a}{b} ")))
        .collect::<String>()
        .chars()
        .take(300)
        .collect();
    let cases: Vec<(&str, String)> = vec![
        ("1 char", "p".into()),
        ("3 chars", "pln".into()),
        ("8 chars", "projplan".into()),
        ("two terms", "plan jour".into()),
        ("unaccented over accented", "ete".into()),
        ("e e e e e e e e", "e e e e e e e e".into()),
        ("300 chars of `e `", "e ".repeat(150)),
        ("300 chars of distinct terms", distinct),
    ];
    bench_cases(c, "base", &names, &cases);

    let recent: Vec<EntryId> = ENTRIES
        .iter()
        .filter(|(_, _, kind, _)| *kind == Kind::Note)
        .step_by(2_500)
        .map(|(id, ..)| *id)
        .take(20)
        .collect();
    let query = NameQuery::new("", None, &recent);
    let cancel = AtomicBool::new(false);
    c.bench_function(
        "match_names/50k base, empty query with a 20-note MRU",
        |b| {
            b.iter(|| black_box(match_names(&names, 0..names.len(), &query, &cancel, LIMIT)));
        },
    );
}

fn bench_realistic(c: &mut Criterion) {
    let cases: Vec<(&str, String)> = vec![
        ("ete", "ete".into()),
        ("pln", "pln".into()),
        ("two terms", "plan jour".into()),
        ("aeiou", "aeiou".into()),
    ];
    let names = NoteSet::build(long_stems()).names();
    bench_cases(c, "long stems", &names, &cases);
    let names = NoteSet::build(deep()).names();
    bench_cases(c, "deep", &names, &cases);
}

/// The quick-open delegate's split: one range per thread, then the merge.
fn parallel(names: &NamesSnapshot, query: &NameQuery, threads: usize) -> Vec<NameMatch> {
    let cancel = &AtomicBool::new(false);
    let found = std::thread::scope(|scope| {
        let scans: Vec<_> = names
            .chunks(threads)
            .into_iter()
            .map(|range| scope.spawn(move || match_names(names, range, query, cancel, LIMIT)))
            .collect();
        scans
            .into_iter()
            .map(|scan| scan.join().unwrap_or_default())
            .collect()
    });
    merge_names(found, LIMIT)
}

fn bench_parallel(c: &mut Criterion) {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut group = c.benchmark_group(format!("parallel, {threads} threads"));
    for (vault, entries) in [("long stems", long_stems()), ("deep", deep())] {
        let names = NoteSet::build(entries).names();
        for text in ["ete", "aeiou", "pln"] {
            let query = NameQuery::new(text, None, &[]);
            group.bench_function(format!("{vault} {text}"), |b| {
                b.iter(|| black_box(parallel(&names, &query, threads)));
            });
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_build,
    bench_set_notes,
    bench_match,
    bench_realistic,
    bench_parallel
);
criterion_main!(benches);
