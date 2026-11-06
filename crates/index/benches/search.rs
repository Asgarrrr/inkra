use std::collections::HashMap;
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{
    IndexSnapshot, MergeWhen, NoteContent, NoteIndex, NoteQuery, NoteSet, Read, Searched,
    TextSearch,
};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Mtime, Stamp, Vault};

/// The stamp of every text the bench parses: nothing here compares stamps.
const EPOCH: Stamp = Stamp {
    mtime: Mtime(UNIX_EPOCH),
    size: 0,
};

const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
const LIMIT: usize = 1_000;
const WORDS: &[&str] = &[
    "le", "la", "les", "de", "des", "et", "un", "une", "projet", "projets", "réunion", "été",
    "équipe", "idée", "café", "santé", "déjà", "très", "être", "après", "the", "and", "of", "to",
    "project", "meeting", "summer", "team", "idea", "coffee", "notes", "plan", "draft", "review",
    "roadmap", "budget", "voyage", "lecture", "journal", "archive",
];
/// One note in 1,000 holds it once.
const RARE: &str = "zanzibar";
/// Scanned once: a `MemFs` scan cost grows with files per folder times folders.
static ENTRIES: LazyLock<Vec<(EntryId, String, Kind, Mtime)>> = LazyLock::new(entries);

/// A fixed LCG, so every run benches the same corpus.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(self.0 >> 33).unwrap_or(0) % below
    }
}

fn entries() -> Vec<(EntryId, String, Kind, Mtime)> {
    let fs = MemFs::new();
    for note in 0..NOTES {
        fs.insert_file(format!("/vault/f{}/n{note}.md", note % FOLDERS), "");
    }
    let start = Instant::now();
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new("/vault"))) else {
        panic!("a MemFs scan cannot fail");
    };
    eprintln!("MemFs scan of {NOTES} notes: {:?}", start.elapsed());
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

/// 1 to 5 KB of prose: lines of 8 to 20 words, `.` or `,` between some.
fn note_text(lcg: &mut Lcg, note: usize) -> String {
    let target = 1_024 + lcg.next(4_097);
    let mut text = String::with_capacity(target + 128);
    while text.len() < target {
        for word in 0..8 + lcg.next(13) {
            if word > 0 {
                text.push(if lcg.next(9) == 0 { ',' } else { ' ' });
            }
            text.push_str(WORDS[lcg.next(WORDS.len())]);
        }
        text.push_str(".\n");
    }
    if note % 1_000 == 7 {
        text.push_str(RARE);
    }
    text
}

/// The text each loaded note holds in memory, as the search reads it.
type Texts = HashMap<EntryId, Arc<str>>;

/// Every note loaded, and its text; also returns one note and its text for the slot write.
fn index() -> (NoteIndex, Texts, u64, (EntryId, Arc<str>)) {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    let mut lcg = Lcg(13);
    let mut total = 0;
    let mut texts = Texts::new();
    let mut last = None;
    let notes = ENTRIES.iter().filter(|(_, _, kind, _)| *kind == Kind::Note);
    for (note, (id, ..)) in notes.enumerate() {
        let text: Arc<str> = note_text(&mut lcg, note).into();
        total += text.len();
        index.insert(*id, NoteContent::parse(&text, EPOCH));
        texts.insert(*id, Arc::clone(&text));
        last = Some((*id, text));
    }
    let Some(last) = last else {
        panic!("the vault holds {NOTES} notes");
    };
    settle(&mut index);
    (index, texts, u64::try_from(total).unwrap_or(u64::MAX), last)
}

/// Merges every fresh note's words, as the app does once loads calm down.
fn settle(index: &mut NoteIndex) {
    while let Some(merge) = index.words_merge(MergeWhen::Settle) {
        let _ = index.words_apply(merge.run());
    }
    assert!(index.words_settled(), "the words are merged");
}

/// One keystroke on one core: the candidates, then each one's text in memory searched.
fn keystroke(
    snapshot: &IndexSnapshot,
    texts: &Texts,
    query: &NoteQuery,
    cancel: &AtomicBool,
) -> Searched {
    let Some(candidates) = snapshot.candidates(query, cancel) else {
        panic!("not cancelled");
    };
    let mut search = TextSearch::new(query, LIMIT);
    for candidate in &candidates {
        let Some(text) = texts.get(&candidate.id) else {
            panic!("a candidate has a text");
        };
        let read = Read::Text {
            text: Arc::clone(text),
            stamp: candidate.stamp,
        };
        if !search.read(candidate, read, cancel) {
            break;
        }
    }
    let Some(found) = search.finish(cancel) else {
        panic!("not cancelled");
    };
    found.searched
}

fn hit_count(found: &Searched) -> usize {
    found.notes.iter().map(|group| group.hits.len()).sum()
}

fn bench_search(c: &mut Criterion) {
    let (mut index, texts, total, (id, text)) = index();
    let corpus = index.corpus();
    eprintln!(
        "corpus: {} notes, {total} text bytes, heap_bytes {}",
        corpus.len(),
        corpus.heap_bytes()
    );

    let cancel = AtomicBool::new(false);
    let snapshot = index.snapshot();
    let mut group = c.benchmark_group("search/50k notes");
    group.throughput(Throughput::Bytes(total));
    group.sample_size(10);
    for (case, needle) in [
        ("rare word", "zanzibar"),
        ("common word", "projet"),
        ("1 char, hits the cap", "e"),
        ("frequent prefix, no match", "projetzz"),
        ("folded word", "ete"),
        ("1 accented char", "é"),
        ("accented word", "réunion"),
        ("two words", "projet réunion"),
        ("1 char twice", "e e"),
    ] {
        let query = NoteQuery::new(needle, None, &[]);
        let hits = hit_count(&keystroke(&snapshot, &texts, &query, &cancel));
        group.bench_function(format!("{case}, {needle:?}, {hits} hits"), |b| {
            b.iter(|| black_box(keystroke(&snapshot, &texts, &query, &cancel)));
        });
    }
    group.finish();
    drop(corpus);
    drop(snapshot);

    let mut group = c.benchmark_group("snapshot/50k notes");
    group.bench_function("corpus()", |b| b.iter(|| black_box(index.corpus())));
    group.bench_function("insert, a snapshot held", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let held = index.corpus();
                let content = NoteContent::parse(&text, EPOCH);
                let start = Instant::now();
                index.insert(id, content);
                total += start.elapsed();
                drop(held);
            }
            total
        });
    });
    group.finish();
}

/// `copies` notes sharing `bytes` of prose then `tail`, with a line break every `line` bytes,
/// or none.
fn long_notes(
    words: &[&str],
    bytes: usize,
    line: Option<usize>,
    copies: usize,
    tail: &str,
) -> (NoteIndex, Texts, u64) {
    let fs = MemFs::new();
    for copy in 0..copies {
        fs.insert_file(format!("/vault/long{copy}.md"), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new("/vault"))) else {
        panic!("a MemFs scan cannot fail");
    };
    let entries: Vec<_> = vault
        .entries()
        .map(|entry| {
            (
                entry.id,
                entry.path.as_path().to_string_lossy().into_owned(),
                entry.kind,
                entry.mtime,
            )
        })
        .collect();
    let ids: Vec<EntryId> = entries.iter().map(|entry| entry.0).collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries));
    let mut lcg = Lcg(5);
    let mut text = String::with_capacity(bytes + 64);
    let mut since = 0;
    while text.len() < bytes {
        let word = words[lcg.next(words.len())];
        text.push_str(word);
        since += word.len() + 1;
        if line.is_some_and(|line| since >= line) {
            text.push('\n');
            since = 0;
        } else {
            text.push(' ');
        }
    }
    text.push_str(tail);
    let total = u64::try_from(text.len() * copies).unwrap_or(u64::MAX);
    let text: Arc<str> = text.into();
    let mut texts = Texts::new();
    for id in ids {
        index.insert(id, NoteContent::parse(&text, EPOCH));
        texts.insert(id, Arc::clone(&text));
    }
    settle(&mut index);
    (index, texts, total)
}

fn bench_long_line(c: &mut Criterion) {
    let cancel = AtomicBool::new(false);
    let mut group = c.benchmark_group("long lines");
    group.sample_size(10);
    for (case, line, bytes, copies) in [
        ("one 10 MB note, lines of 80 bytes", Some(80), 10 << 20, 1),
        ("one 10 MB note, one line", None, 10 << 20, 1),
        ("ten 1 MB notes, lines of 80 bytes", Some(80), 1 << 20, 10),
        ("ten 1 MB notes, one line each", None, 1 << 20, 10),
    ] {
        let (index, texts, total) = long_notes(WORDS, bytes, line, copies, "");
        let snapshot = index.snapshot();
        group.throughput(Throughput::Bytes(total));
        for needle in ["projet", "zanzibar"] {
            let query = NoteQuery::new(needle, None, &[]);
            group.bench_function(format!("{case}, {needle}"), |b| {
                b.iter(|| black_box(keystroke(&snapshot, &texts, &query, &cancel)));
            });
        }
    }
    // The index lists the note, so the walk reads nearly all of it for one hit.
    let (index, texts, total) = long_notes(WORDS, 10 << 20, Some(80), 1, "zanzibar.\n");
    let snapshot = index.snapshot();
    let query = NoteQuery::new("zanzibar", None, &[]);
    assert_eq!(hit_count(&keystroke(&snapshot, &texts, &query, &cancel)), 1);
    group.throughput(Throughput::Bytes(total));
    group.bench_function(
        "one 10 MB note, lines of 80 bytes, zanzibar once at its end",
        |b| {
            b.iter(|| black_box(keystroke(&snapshot, &texts, &query, &cancel)));
        },
    );
    group.finish();
}

const CYRILLIC: &[&str] = &[
    "проект",
    "встреча",
    "лето",
    "команда",
    "идея",
    "кофе",
    "заметки",
    "план",
    "и",
    "в",
    "не",
    "на",
    "что",
    "это",
    "бюджет",
    "обзор",
];
const GREEK: &[&str] = &[
    "σχέδιο",
    "συνάντηση",
    "καλοκαίρι",
    "ομάδα",
    "ιδέα",
    "καφές",
    "σημειώσεις",
    "και",
    "το",
    "της",
    "προϋπολογισμός",
    "ανασκόπηση",
];
const CJK: &[&str] = &[
    "我们的",
    "项目",
    "会议",
    "夏天",
    "团队",
    "想法",
    "咖啡",
    "笔记",
    "计划",
    "预算",
];

fn bench_scripts(c: &mut Criterion) {
    let cancel = AtomicBool::new(false);
    let mut group = c.benchmark_group("scripts, 10 MB, lines of 80 bytes");
    group.sample_size(10);
    for (script, words, needles) in [
        ("cyrillic", CYRILLIC, &["е", "о", "ет", "зzz"][..]),
        ("greek", GREEK, &["σ", "α", "ο"][..]),
        ("cjk", CJK, &["的", "项", "会"][..]),
    ] {
        let (index, texts, total) = long_notes(words, 10 << 20, Some(80), 1, "");
        let snapshot = index.snapshot();
        group.throughput(Throughput::Bytes(total));
        for &needle in needles {
            let query = NoteQuery::new(needle, None, &[]);
            let hits = hit_count(&keystroke(&snapshot, &texts, &query, &cancel));
            group.bench_function(format!("{script}, {needle}, {hits} hits"), |b| {
                b.iter(|| black_box(keystroke(&snapshot, &texts, &query, &cancel)));
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_search, bench_long_line, bench_scripts);
criterion_main!(benches);
