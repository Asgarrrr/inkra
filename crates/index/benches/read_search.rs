//! `⌘⇧F` over a `bench_vault` vault on disk: the baseline a read-through search is gated against.

use std::collections::{BTreeMap, HashSet};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{MergeWhen, NoteContent, NoteIndex, NoteQuery, NoteSet};
use inkra_vault::{Fs as _, Kind, RealFs, Serial, Vault};

const LIMIT: usize = 1_000;
const TARGET_DF: usize = 1_000;
const MIN_TOKEN_CHARS: usize = 4;

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
            NoteContent::parse(loaded.text.into(), loaded.stamp.mtime),
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
    let mut group = c.benchmark_group("read_search");
    group.sample_size(10);
    group.bench_function("resident", |b| {
        b.iter(|| black_box(block_on(snapshot.search(&query, LIMIT, &Serial, &cancel))));
    });
    group.finish();
}

criterion_group!(benches, bench_read_search);
criterion_main!(benches);
