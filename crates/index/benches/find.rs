#[path = "words/corpus.rs"]
mod corpus;
#[path = "names/paths.rs"]
mod paths;

use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::UNIX_EPOCH;

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{
    Excerpt, Group, IndexSnapshot, MergeWhen, NoteContent, NoteIndex, NoteMatch, NoteQuery,
    NoteSet, excerpt,
};
use inkra_vault::{EntryId, Job, Jobs, Kind, Mtime, Serial, Stamp};

use corpus::{Corpus, NOTE_IDS, NOTES, contents};
use paths::{Lcg, folders};

/// The stamp of every text the bench parses: nothing here compares stamps.
const EPOCH: Stamp = Stamp {
    mtime: Mtime(UNIX_EPOCH),
    size: 0,
};

/// Quick open's `LIMIT`.
const LIMIT: usize = 100;
const THREADS: usize = 10;
/// Quick open splits into one range per core.
const PIPELINE_RANGES: usize = THREADS;
/// Quick open's rows on screen: only these get an excerpt.
const ON_SCREEN: usize = 10;
const RECENT: usize = 20;
/// A word of the long-stem names that no text of either corpus holds.
const NAME_ONLY: &str = "workshop";

const KINDS: &[&str] = &["Meeting", "Call", "Lunch", "Review", "Workshop"];
const PEOPLE: &[&str] = &[
    "Hélène", "Marc", "Zoé", "Jérôme", "Anaïs", "Paul", "Léa", "Thomas",
];
const TOPICS: &[&str] = &[
    "the budget",
    "the roadmap",
    "l'été",
    "la réunion",
    "the design",
    "l'équipe",
    "la santé",
    "the trip",
];

/// About 56 chars: two or three folders, then "Meeting with Zoé about the roadmap 12".
fn long_stem(lcg: &mut Lcg, note: usize) -> String {
    let depth = 2 + lcg.next(2);
    let folders = folders(lcg, depth);
    let kind = KINDS[lcg.next(KINDS.len())];
    let person = PEOPLE[lcg.next(PEOPLE.len())];
    let topic = TOPICS[lcg.next(TOPICS.len())];
    format!("{folders}/{kind} with {person} about {topic} {note}.md")
}

/// Each note's text, as the readers give it to `excerpt`.
type Texts = HashMap<EntryId, Arc<str>>;

fn indexed(corpus: Corpus) -> (IndexSnapshot, Texts) {
    let mut lcg = Lcg(11);
    let entries = NOTE_IDS
        .iter()
        .enumerate()
        .map(|(note, &id)| (id, long_stem(&mut lcg, note), Kind::Note, Mtime(UNIX_EPOCH)))
        .collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries));
    let texts = corpus.texts();
    for (&id, content) in NOTE_IDS.iter().zip(contents(&texts)) {
        index.insert(id, content);
    }
    let texts = NOTE_IDS.iter().copied().zip(texts).collect();
    (settle(&mut index), texts)
}

fn settle(index: &mut NoteIndex) -> IndexSnapshot {
    while let Some(merge) = index.words_merge(MergeWhen::Settle) {
        let _ = index.words_apply(merge.run());
    }
    assert!(index.words_settled(), "the words are merged");
    index.snapshot()
}

fn cases(corpus: Corpus) -> Vec<(String, NoteQuery)> {
    let recent: Vec<EntryId> = NOTE_IDS.iter().step_by(NOTES / RECENT).copied().collect();
    let mut cases: Vec<(String, NoteQuery)> = corpus
        .queries()
        .into_iter()
        .map(|(case, text)| (case.to_owned(), NoteQuery::new(&text, None, &[])))
        .collect();
    cases.push((
        format!("name only {NAME_ONLY:?}"),
        NoteQuery::new(NAME_ONLY, None, &[]),
    ));
    cases.push((
        format!("empty, {RECENT} recent"),
        NoteQuery::new("", None, &recent),
    ));
    cases
}

/// `ranges` jobs drained by `THREADS` threads.
struct Threads {
    ranges: usize,
}

impl Jobs for Threads {
    fn parallelism(&self) -> usize {
        self.ranges
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            let queue = &Mutex::new(jobs.into_iter());
            let next = || queue.lock().unwrap_or_else(PoisonError::into_inner).next();
            thread::scope(|scope| {
                for _ in 0..THREADS {
                    scope.spawn(|| {
                        while let Some(job) = next() {
                            block_on(job);
                        }
                    });
                }
            });
        })
    }
}

/// `find_notes` on `jobs`.
fn find(snapshot: &IndexSnapshot, query: &NoteQuery, jobs: &dyn Jobs) -> Vec<NoteMatch> {
    let Some(found) = block_on(snapshot.find_notes(query, LIMIT, jobs, &AtomicBool::new(false)))
    else {
        panic!("not cancelled");
    };
    found
}

fn split(snapshot: &IndexSnapshot, query: &NoteQuery, ranges: usize) -> Vec<NoteMatch> {
    find(snapshot, query, &Threads { ranges })
}

fn excerpts(
    snapshot: &IndexSnapshot,
    texts: &Texts,
    rows: &[NoteMatch],
    query: &NoteQuery,
) -> Vec<Option<Excerpt>> {
    let cancel = AtomicBool::new(false);
    rows.iter()
        .map(|found| excerpt(snapshot, found, query, &texts[&found.id], &cancel))
        .collect()
}

/// The first `ON_SCREEN` rows' excerpts, one thread each, as quick open spreads them.
fn on_screen(
    snapshot: &IndexSnapshot,
    texts: &Texts,
    rows: &[NoteMatch],
    query: &NoteQuery,
) -> Vec<Option<Excerpt>> {
    let cancel = &AtomicBool::new(false);
    thread::scope(|scope| {
        let workers: Vec<_> = rows
            .iter()
            .take(ON_SCREEN)
            .map(|found| {
                let text = &texts[&found.id];
                scope.spawn(move || excerpt(snapshot, found, query, text, cancel))
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

/// Up to the rows: the lookup, the split, the merge.
fn to_names(snapshot: &IndexSnapshot, query: &NoteQuery) -> Vec<NoteMatch> {
    split(snapshot, query, PIPELINE_RANGES)
}

/// Then `quick_open/list.rs`'s: the excerpts of the rows on screen.
fn to_excerpts(snapshot: &IndexSnapshot, texts: &Texts, query: &NoteQuery) -> Vec<Option<Excerpt>> {
    on_screen(snapshot, texts, &to_names(snapshot, query), query)
}

fn report(corpus: Corpus, case: &str, snapshot: &IndexSnapshot, texts: &Texts, query: &NoteQuery) {
    let rows = split(snapshot, query, PIPELINE_RANGES);
    let count = |group| rows.iter().filter(|found| found.group == group).count();
    let lines = excerpts(snapshot, texts, &rows, query)
        .iter()
        .flatten()
        .count();
    eprintln!(
        "find, {}, {case}: {} rows ({} name, {} name and text, {} text), {lines} second lines",
        corpus.name(),
        rows.len(),
        count(Group::Name),
        count(Group::NameAndText),
        count(Group::Text),
    );
}

fn bench_corpus(c: &mut Criterion, corpus: Corpus) {
    let (snapshot, texts) = indexed(corpus);
    let cases = cases(corpus);
    for (case, query) in &cases {
        report(corpus, case, &snapshot, &texts, query);
    }

    let mut group = c.benchmark_group(format!("find_notes, {}, one core", corpus.name()));
    for (case, query) in &cases {
        group.bench_function(case, |b| {
            b.iter(|| black_box(find(&snapshot, query, &Serial)));
        });
    }
    group.finish();

    for per_thread in [1, 2, 4] {
        let mut group = c.benchmark_group(format!(
            "find_notes, {}, {THREADS} threads, {per_thread}× ranges",
            corpus.name()
        ));
        for (case, query) in &cases {
            group.bench_function(case, |b| {
                b.iter(|| black_box(split(&snapshot, query, per_thread * THREADS)));
            });
        }
        group.finish();
    }

    let mut group = c.benchmark_group(format!("excerpt, {}, top {LIMIT} rows", corpus.name()));
    for (case, query) in &cases {
        let rows = split(&snapshot, query, PIPELINE_RANGES);
        group.bench_function(case, |b| {
            b.iter(|| black_box(excerpts(&snapshot, &texts, &rows, query)));
        });
    }
    group.finish();

    let mut group = c.benchmark_group(format!("keystroke to names, {}", corpus.name()));
    for (case, query) in &cases {
        group.bench_function(case, |b| {
            b.iter(|| black_box(to_names(&snapshot, query)));
        });
    }
    group.finish();

    let mut group = c.benchmark_group(format!(
        "keystroke to excerpts, {}, {ON_SCREEN} on screen",
        corpus.name()
    ));
    for (case, query) in &cases {
        group.bench_function(case, |b| {
            b.iter(|| black_box(to_excerpts(&snapshot, &texts, query)));
        });
    }
    group.finish();
}

fn bench_zipf(c: &mut Criterion) {
    bench_corpus(c, Corpus::Zipf);
}

fn bench_forty(c: &mut Criterion) {
    bench_corpus(c, Corpus::Forty);
}

/// Two notes that make the excerpt's scan read its whole limit.
fn bench_adversarial(c: &mut Criterion) {
    let notes: [(&str, &str, String); 2] = [
        (
            "1 MB of `-`, then a 1 MB word of `é`",
            "ééé",
            format!("{}{}", "-".repeat(1_000_000), "é".repeat(500_000)),
        ),
        (
            "a 10 MB line, the match at its end",
            "plan",
            format!("{}plan", "xy ".repeat(10_000_000 / 3)),
        ),
    ];
    let entries = NOTE_IDS
        .iter()
        .zip(["adversarial/wide.md", "adversarial/long.md"])
        .map(|(&id, path)| (id, path.to_owned(), Kind::Note, Mtime(UNIX_EPOCH)))
        .collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries));
    for (&id, (_, _, text)) in NOTE_IDS.iter().zip(&notes) {
        index.insert(id, NoteContent::parse(text, EPOCH));
    }
    let snapshot = settle(&mut index);
    let cancel = AtomicBool::new(false);
    let mut group = c.benchmark_group("excerpt, adversarial");
    for (case, query, text) in &notes {
        let query = NoteQuery::new(query, None, &[]);
        let rows = find(&snapshot, &query, &Serial);
        let [found] = rows.as_slice() else {
            panic!("{case}: one row, found {}", rows.len());
        };
        let shown = excerpt(&snapshot, found, &query, text, &cancel);
        eprintln!(
            "excerpt, adversarial, {case}: {:?} group, {:?} bytes shown",
            found.group,
            shown.map(|shown| shown.text.len())
        );
        group.bench_function(*case, |b| {
            b.iter(|| black_box(excerpt(&snapshot, found, &query, text, &cancel)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_zipf, bench_forty, bench_adversarial);
criterion_main!(benches);
