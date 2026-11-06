mod build;
mod corpus;

use std::collections::HashSet;
use std::hint::black_box;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, SamplingMode, criterion_group, criterion_main};
use inkra_index::{
    CorpusSnapshot, Lookup, MergeWhen, NoteContent, NoteIndex, NoteQuery, NoteSet, WordsSnapshot,
};
use inkra_vault::{EntryId, Kind, Mtime};

use build::{FRESH_MIN, FRESH_RATIO, build, loaded_notes, max_fresh, settle};
use corpus::{
    Corpus, ENTRIES, EPOCH, Lcg, NOTE_IDS, NOTES, Pick, contents, forty_word, prose, zipf_word,
};

/// A trash of this many notes while loaded.
const GONE: usize = 5_500;
/// Notes rewritten before `merge/12.5k re-set into 50k`.
const RESET: usize = 12_500;

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

fn greek_word(lcg: &mut Lcg) -> &'static str {
    GREEK[lcg.next(GREEK.len())]
}

/// A note holding a 1 MB base64 paste on one line: tens of thousands of distinct words.
fn base64_note() -> Arc<str> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lcg = Lcg(17);
    let mut text = String::from("A screenshot: ![shot](data:image/png;base64,");
    while text.len() < 1 << 20 {
        text.push(char::from(ALPHABET[lcg.next(ALPHABET.len())]));
    }
    text.push_str(")\n");
    text.into()
}

fn rss_kb() -> u64 {
    let pid = std::process::id().to_string();
    let Ok(out) = Command::new("ps").args(["-o", "rss=", "-p", &pid]).output() else {
        panic!("ps runs");
    };
    let Ok(kb) = String::from_utf8_lossy(&out.stdout).trim().parse() else {
        panic!("ps prints the RSS in kB");
    };
    kb
}

/// The 40-word corpus first: memory freed by a build lowers the next build's RSS delta. Each
/// note is parsed just before its insert, as a load lands, so no parsed words pile up.
fn bench_memory(_: &mut Criterion) {
    for corpus in [Corpus::Forty, Corpus::Zipf] {
        let texts = corpus.texts();
        let mut index = loaded_notes();
        let before = rss_kb();
        let parsed = texts
            .iter()
            .map(|text| NoteContent::parse(Arc::clone(text), EPOCH));
        let _ = build(&mut index, parsed);
        let after = rss_kb();
        let (notes, heap_bytes) = (index.corpus().len(), index.words().heap_bytes());
        drop(index);
        eprintln!(
            "words ({}): {notes} notes, heap_bytes {heap_bytes}, RSS {before} → {after} kB, \
             {} kB once the index is dropped",
            corpus.name(),
            rss_kb(),
        );
    }
}

fn bench_parse(c: &mut Criterion) {
    let scripts: [(&str, Pick); 3] = [
        ("zipf", zipf_word),
        ("40 words", forty_word),
        ("greek", greek_word),
    ];
    let mut group = c.benchmark_group("parse/one note of 3 KB");
    for (script, pick) in scripts {
        let text: Arc<str> = prose(&mut Lcg(29), 3_072, pick).into();
        group.bench_function(script, |b| {
            b.iter_with_large_drop(|| NoteContent::parse(Arc::clone(&text), EPOCH));
        });
    }
    group.finish();
}

/// Iterations that each cost a build, a merge or a restore: ten samples of one iteration.
fn expensive(c: &mut Criterion, name: String) -> BenchmarkGroup<'_, WallTime> {
    let mut group = c.benchmark_group(name);
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(1))
        .measurement_time(Duration::from_millis(1));
    group
}

fn bench_build(c: &mut Criterion, corpus: Corpus, texts: &[Arc<str>]) {
    let mut group = expensive(c, format!("build/50k notes, {}", corpus.name()));
    group.bench_function("foreground", |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| build(&mut loaded_notes(), contents(texts)).foreground())
                .sum()
        });
    });
    group.finish();
}

fn bench_lookups(c: &mut Criterion, corpus: Corpus, state: &str, index: &NoteIndex) {
    let words = index.words();
    let cancel = AtomicBool::new(false);
    let mut group = c.benchmark_group(format!("lookup, {}, {state}", corpus.name()));
    group.sample_size(20);
    for (case, text) in corpus.queries() {
        let query = NoteQuery::new(&text, None, &[]);
        let Some(found) = words.lookup(&query, &cancel) else {
            panic!("not cancelled");
        };
        let notes = NOTE_IDS
            .iter()
            .filter(|&&id| found.get(0, id).is_some())
            .count();
        eprintln!(
            "lookup/{case}, {}, {state}: {text:?}, its first term in {notes} notes",
            corpus.name()
        );
        group.bench_function(case, |b| {
            b.iter(|| black_box(words.lookup(&query, &cancel)));
        });
    }
    group.finish();
}

/// What is held across an insert, as quick open holds it during a search.
#[derive(Clone, Copy)]
enum Hold {
    Nothing,
    Words,
    Both,
    Lookup,
}

type Held = (
    Option<WordsSnapshot>,
    Option<CorpusSnapshot>,
    Option<Lookup>,
);

impl Hold {
    fn take(self, index: &NoteIndex) -> Held {
        match self {
            Self::Nothing => (None, None, None),
            Self::Words => (Some(index.words()), None, None),
            Self::Both => (Some(index.words()), Some(index.corpus()), None),
            // A query matching nothing: the lookup is cheap to build and still holds the snapshot.
            Self::Lookup => {
                let query = NoteQuery::new("zzzz", None, &[]);
                let lookup = index.words().lookup(&query, &AtomicBool::new(false));
                (None, None, lookup)
            }
        }
    }
}

/// `iters` inserts of `text` as `id`, each timed alone; `new` removes the note first, untimed,
/// so the insert gives it an ordinal.
fn inserts(
    index: &mut NoteIndex,
    iters: u64,
    (id, text): (EntryId, &Arc<str>),
    hold: Hold,
    new: bool,
) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iters {
        if new {
            index.remove(id);
        }
        let content = NoteContent::parse(Arc::clone(text), EPOCH);
        let snapshots = hold.take(index);
        let started = Instant::now();
        index.insert(id, content);
        total += started.elapsed();
        drop(snapshots);
    }
    total
}

fn bench_inserts(
    c: &mut Criterion,
    corpus: Corpus,
    state: &str,
    index: &mut NoteIndex,
    texts: &[Arc<str>],
) {
    let note = (NOTE_IDS[NOTES / 2], &texts[NOTES / 2]);
    let mut group = c.benchmark_group(format!("insert, {}, {state}", corpus.name()));
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(50)
        .warm_up_time(Duration::from_millis(5))
        .measurement_time(Duration::from_millis(20));
    for (case, hold, new) in [
        ("one note, no snapshot", Hold::Nothing, false),
        ("one note, a snapshot held", Hold::Words, false),
        ("one note, both snapshots held", Hold::Both, false),
        ("a new note, a snapshot held", Hold::Words, true),
        ("a new note, a Lookup held", Hold::Lookup, true),
    ] {
        group.bench_function(case, |b| {
            b.iter_custom(|iters| inserts(index, iters, note, hold, new));
        });
    }
    let image = base64_note();
    let note = (NOTE_IDS[1], &image);
    group.measurement_time(Duration::from_millis(1));
    group.bench_function("a 1 MB base64 image, a snapshot held", |b| {
        b.iter_custom(|iters| inserts(index, iters, note, Hold::Words, false));
    });
    group.finish();
}

/// Brings the first `GONE` notes back as new ordinals and settles.
fn restore(index: &mut NoteIndex, texts: &[Arc<str>]) {
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    for (&id, content) in NOTE_IDS[..GONE].iter().zip(contents(&texts[..GONE])) {
        index.insert(id, content);
    }
    settle(index);
}

/// `iters` `set_notes` of `notes`, each timed alone; the set it returns is freed untimed, as
/// the app frees it off the foreground.
fn set_notes(
    index: &mut NoteIndex,
    iters: u64,
    notes: &[(EntryId, String, Kind, Mtime)],
    held: bool,
    texts: &[Arc<str>],
) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iters {
        let set = NoteSet::build(notes.to_vec());
        let held = held.then(|| index.words());
        let started = Instant::now();
        let previous = index.set_notes(set);
        total += started.elapsed();
        drop((held, previous));
        if notes.len() < ENTRIES.len() {
            restore(index, texts);
        }
    }
    total
}

fn bench_set_notes(c: &mut Criterion, corpus: Corpus, texts: &[Arc<str>]) {
    let mut index = loaded_notes();
    let _ = build(&mut index, contents(texts));
    let gone: HashSet<EntryId> = NOTE_IDS[..GONE].iter().copied().collect();
    let kept: Vec<_> = ENTRIES
        .iter()
        .filter(|(id, ..)| !gone.contains(id))
        .cloned()
        .collect();
    let _ = index.set_notes(NoteSet::build(kept.clone()));
    restore(&mut index, texts);
    eprintln!(
        "ordinals ({}): span {} for {NOTES} notes after {GONE} removed and re-inserted",
        corpus.name(),
        index.words().ordinal_span()
    );
    let mut group = expensive(c, format!("set_notes, {}", corpus.name()));
    for (case, notes, held) in [
        ("nothing gone", &ENTRIES[..], false),
        ("5,500 gone", &kept[..], false),
        ("5,500 gone, a snapshot held", &kept[..], true),
    ] {
        group.bench_function(case, |b| {
            b.iter_custom(|iters| set_notes(&mut index, iters, notes, held, texts));
        });
    }
    group.finish();
}

/// `iters` runs of the merge `index` starts, each dropped unapplied so the next starts again.
fn runs(index: &mut NoteIndex, iters: u64) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iters {
        let Some(merge) = index.words_merge(MergeWhen::Settle) else {
            panic!("fresh notes to merge");
        };
        let started = Instant::now();
        let merged = merge.run();
        total += started.elapsed();
        drop(merged);
    }
    total
}

fn bench_merges(c: &mut Criterion, corpus: Corpus, texts: &[Arc<str>]) {
    let mut index = loaded_notes();
    for (&id, content) in NOTE_IDS.iter().zip(contents(texts)) {
        index.insert(id, content);
    }
    let mut group = expensive(c, format!("merge, {}", corpus.name()));
    group.bench_function("50k fresh into empty", |b| {
        b.iter_custom(|iters| runs(&mut index, iters));
    });
    settle(&mut index);
    for (&id, content) in NOTE_IDS[..RESET].iter().zip(contents(&texts[..RESET])) {
        index.insert(id, content);
    }
    group.bench_function("12.5k re-set into 50k", |b| {
        b.iter_custom(|iters| runs(&mut index, iters));
    });
    group.finish();
}

fn bench_corpus(c: &mut Criterion, corpus: Corpus) {
    let texts = corpus.texts();
    let mut settled = loaded_notes();
    build(&mut settled, contents(&texts)).report(corpus);
    bench_build(c, corpus, &texts);
    let mut fresh = max_fresh(&texts);
    let tier = format!("{} fresh", FRESH_MIN.max(NOTES / FRESH_RATIO));
    for (state, index) in [("settled", &mut settled), (tier.as_str(), &mut fresh)] {
        bench_lookups(c, corpus, state, index);
        bench_inserts(c, corpus, state, index, &texts);
    }
    drop((settled, fresh));
    bench_set_notes(c, corpus, &texts);
    bench_merges(c, corpus, &texts);
}

fn bench_zipf(c: &mut Criterion) {
    bench_corpus(c, Corpus::Zipf);
}

fn bench_forty(c: &mut Criterion) {
    bench_corpus(c, Corpus::Forty);
}

criterion_group!(benches, bench_memory, bench_parse, bench_zipf, bench_forty);
criterion_main!(benches);
