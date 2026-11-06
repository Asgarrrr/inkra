//! `⌘⇧F` over a `bench_vault` vault on disk: resident texts against reads on `READERS` threads.

use std::collections::{BTreeMap, HashSet};
use std::hint::black_box;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::thread;

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{
    Candidate, IndexSnapshot, MergeWhen, NoteContent, NoteIndex, NoteQuery, NoteSet, Read,
    TextFound, TextSearch,
};
use inkra_vault::{Fs as _, Kind, RealFs, Serial, Vault, VaultError};

const LIMIT: usize = 1_000;
const TARGET_DF: usize = 1_000;
const MIN_TOKEN_CHARS: usize = 4;
/// The app's `READERS`.
const READERS: usize = 4;

fn vault_dir() -> PathBuf {
    std::env::var_os("INKRA_BENCH_VAULT").map_or_else(
        || std::env::temp_dir().join("inkra-bench-50000"),
        PathBuf::from,
    )
}

/// Merges every fresh note's words, as the app does once loads calm down.
fn settle(index: &mut NoteIndex) {
    while let Some(merge) = index.words_merge(MergeWhen::Settle) {
        let _ = index.words_apply(merge.run());
    }
    assert!(index.words_settled(), "the words are merged");
}

/// Counts each token once per note; the bench picks its term from these counts.
fn count_tokens(text: &str, df: &mut BTreeMap<String, usize>) {
    let tokens: HashSet<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.chars().count() >= MIN_TOKEN_CHARS)
        .map(str::to_lowercase)
        .collect();
    for token in tokens {
        *df.entry(token).or_default() += 1;
    }
}

/// `candidate` read from disk now, as the app's reader maps it.
fn read(root: &Path, candidate: &Candidate) -> Read {
    match block_on(RealFs.load(&root.join(&*candidate.path))) {
        Ok(loaded) => Read::Text {
            text: loaded.text.into(),
            stamp: loaded.stamp,
        },
        Err(VaultError::NotUtf8 { .. }) => Read::NotUtf8,
        Err(error) if error.io_kind() == Some(ErrorKind::NotFound) => Read::Gone,
        Err(_) => Read::Failed,
    }
}

/// The candidates read by `READERS` threads, a contiguous share each, then searched in order.
fn read4(
    snapshot: &IndexSnapshot,
    root: &Path,
    query: &NoteQuery,
    cancel: &AtomicBool,
) -> Option<TextFound> {
    let candidates = snapshot.candidates(query, cancel)?;
    let share = candidates.len().div_ceil(READERS).max(1);
    let reads: Vec<Read> = thread::scope(|scope| {
        let readers: Vec<_> = candidates
            .chunks(share)
            .map(|chunk| {
                scope.spawn(move || chunk.iter().map(|c| read(root, c)).collect::<Vec<_>>())
            })
            .collect();
        readers
            .into_iter()
            .flat_map(|reader| {
                let Ok(reads) = reader.join() else {
                    panic!("a reader panicked");
                };
                reads
            })
            .collect()
    });
    let mut search = TextSearch::new(query, LIMIT);
    for (candidate, read) in candidates.iter().zip(reads) {
        if !search.read(candidate, read, cancel) {
            break;
        }
    }
    search.finish(cancel)
}

fn bench_read_search(c: &mut Criterion) {
    let root = vault_dir();
    if !root.is_dir() {
        eprintln!(
            "read_search: no vault at {}; run cargo run -p inkra-app --release --example bench_vault -- 50000",
            root.display()
        );
        return;
    }
    let Ok(vault) = block_on(Vault::scan(&RealFs, &root)) else {
        panic!("the vault at {} scans", root.display());
    };
    let entries = vault.entries().map(|entry| {
        (
            entry.id,
            entry.path.as_path().to_string_lossy().into_owned(),
            entry.kind,
            entry.mtime,
        )
    });
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries.collect()));
    let mut df = BTreeMap::new();
    let notes = vault.entries().filter(|entry| entry.kind == Kind::Note);
    for entry in notes {
        let Some(path) = vault.absolute(entry.id) else {
            panic!("a scanned note has a path");
        };
        let Ok(loaded) = block_on(RealFs.load(&path)) else {
            panic!("{} loads", path.display());
        };
        count_tokens(&loaded.text, &mut df);
        index.insert(
            entry.id,
            NoteContent::parse(loaded.text.into(), loaded.stamp),
        );
    }
    settle(&mut index);
    let Some((term, notes)) = df
        .iter()
        .min_by_key(|&(_, &notes)| notes.abs_diff(TARGET_DF))
    else {
        panic!("the vault holds words of {MIN_TOKEN_CHARS} characters or more");
    };
    eprintln!("read_search: term {term}, {notes} notes");

    let query = NoteQuery::new(term, None, &[]);
    let cancel = AtomicBool::new(false);
    let snapshot = index.snapshot();
    let read = read4(&snapshot, &root, &query, &cancel).map(|found| found.searched);
    let resident = block_on(snapshot.search(&query, LIMIT, &Serial, &cancel));
    assert_eq!(read, resident, "the reads find what the resident texts do");
    let mut group = c.benchmark_group("read_search");
    group.sample_size(10);
    group.bench_function("resident", |b| {
        b.iter(|| black_box(block_on(snapshot.search(&query, LIMIT, &Serial, &cancel))));
    });
    group.bench_function("read4", |b| {
        b.iter(|| black_box(read4(&snapshot, &root, &query, &cancel)));
    });
    group.finish();
}

criterion_group!(benches, bench_read_search);
criterion_main!(benches);
