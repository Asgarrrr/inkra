#[path = "words/corpus.rs"]
mod corpus;
#[path = "names/paths.rs"]
mod paths;

use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::UNIX_EPOCH;

use criterion::{Criterion, criterion_group, criterion_main};
use inkra_index::{
    Excerpt, Group, IndexSnapshot, Lookup, MergeWhen, NoteContent, NoteIndex, NoteMatch, NoteQuery,
    NoteSet, excerpt, find_notes, merge_notes,
};
use inkra_vault::{EntryId, Kind, Mtime};

use corpus::{Corpus, NOTE_IDS, NOTES, contents};
use paths::{Lcg, folders};

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

fn indexed(corpus: Corpus) -> IndexSnapshot {
    let mut lcg = Lcg(11);
    let entries = NOTE_IDS
        .iter()
        .enumerate()
        .map(|(note, &id)| (id, long_stem(&mut lcg, note), Kind::Note, Mtime(UNIX_EPOCH)))
        .collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries));
    for (&id, content) in NOTE_IDS.iter().zip(contents(&corpus.texts())) {
        index.insert(id, content);
    }
    settle(&mut index)
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

fn lookup(snapshot: &IndexSnapshot, query: &NoteQuery) -> Lookup {
    let Some(lookup) = snapshot.lookup(query, &AtomicBool::new(false)) else {
        panic!("not cancelled");
    };
    lookup
}

fn split(
    snapshot: &IndexSnapshot,
    lookup: &Lookup,
    query: &NoteQuery,
    ranges: usize,
) -> Vec<NoteMatch> {
    let cancel = &AtomicBool::new(false);
    let ranges = &snapshot.chunks(ranges);
    let next = &AtomicUsize::new(0);
    let found = thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    while let Some(range) = ranges.get(next.fetch_add(1, Ordering::Relaxed)) {
                        found.push(find_notes(
                            snapshot,
                            lookup,
                            range.clone(),
                            query,
                            cancel,
                            LIMIT,
                        ));
                    }
                    found
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    });
    merge_notes(snapshot, found, query, LIMIT)
}

fn excerpts(
    snapshot: &IndexSnapshot,
    rows: &[NoteMatch],
    query: &NoteQuery,
) -> Vec<Option<Excerpt>> {
    let cancel = AtomicBool::new(false);
    rows.iter()
        .map(|found| excerpt(snapshot, found, query, &cancel))
        .collect()
}

/// The first `ON_SCREEN` rows' excerpts, one thread each, as quick open spreads them.
fn on_screen(
    snapshot: &IndexSnapshot,
    rows: &[NoteMatch],
    query: &NoteQuery,
) -> Vec<Option<Excerpt>> {
    let cancel = &AtomicBool::new(false);
    thread::scope(|scope| {
        let workers: Vec<_> = rows
            .iter()
            .take(ON_SCREEN)
            .map(|found| scope.spawn(move || excerpt(snapshot, found, query, cancel)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

/// `index::fan_out`'s order up to the rows: the lookup, the split, the merge.
fn to_names(snapshot: &IndexSnapshot, query: &NoteQuery) -> Vec<NoteMatch> {
    split(snapshot, &lookup(snapshot, query), query, PIPELINE_RANGES)
}

/// Then `quick_open/list.rs`'s: the excerpts of the rows on screen.
fn to_excerpts(snapshot: &IndexSnapshot, query: &NoteQuery) -> Vec<Option<Excerpt>> {
    on_screen(snapshot, &to_names(snapshot, query), query)
}

fn report(corpus: Corpus, case: &str, snapshot: &IndexSnapshot, query: &NoteQuery) {
    let rows = split(snapshot, &lookup(snapshot, query), query, PIPELINE_RANGES);
    let count = |group| rows.iter().filter(|found| found.group == group).count();
    let lines = excerpts(snapshot, &rows, query).iter().flatten().count();
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
    let snapshot = indexed(corpus);
    let cases = cases(corpus);
    let all = 0..NOTES;
    let cancel = AtomicBool::new(false);
    for (case, query) in &cases {
        report(corpus, case, &snapshot, query);
    }

    let mut group = c.benchmark_group(format!("find_notes, {}, one core", corpus.name()));
    for (case, query) in &cases {
        group.bench_function(case, |b| {
            b.iter(|| {
                let lookup = lookup(&snapshot, query);
                let kept = find_notes(&snapshot, &lookup, all.clone(), query, &cancel, LIMIT);
                black_box(merge_notes(&snapshot, vec![kept], query, LIMIT))
            });
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
                b.iter(|| {
                    let lookup = lookup(&snapshot, query);
                    black_box(split(&snapshot, &lookup, query, per_thread * THREADS))
                });
            });
        }
        group.finish();
    }

    let mut group = c.benchmark_group(format!("excerpt, {}, top {LIMIT} rows", corpus.name()));
    for (case, query) in &cases {
        let rows = split(&snapshot, &lookup(&snapshot, query), query, PIPELINE_RANGES);
        group.bench_function(case, |b| {
            b.iter(|| black_box(excerpts(&snapshot, &rows, query)));
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
            b.iter(|| black_box(to_excerpts(&snapshot, query)));
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
        index.insert(
            id,
            NoteContent::parse(text.as_str().into(), Mtime(UNIX_EPOCH)),
        );
    }
    let snapshot = settle(&mut index);
    let cancel = AtomicBool::new(false);
    let mut group = c.benchmark_group("excerpt, adversarial");
    for (case, text, _) in &notes {
        let query = NoteQuery::new(text, None, &[]);
        let kept = find_notes(
            &snapshot,
            &lookup(&snapshot, &query),
            0..notes.len(),
            &query,
            &cancel,
            LIMIT,
        );
        let rows = merge_notes(&snapshot, vec![kept], &query, LIMIT);
        let [found] = rows.as_slice() else {
            panic!("{case}: one row, found {}", rows.len());
        };
        let shown = excerpt(&snapshot, found, &query, &cancel);
        eprintln!(
            "excerpt, adversarial, {case}: {:?} group, {:?} bytes shown",
            found.group,
            shown.map(|shown| shown.text.len())
        );
        group.bench_function(*case, |b| {
            b.iter(|| black_box(excerpt(&snapshot, found, &query, &cancel)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_zipf, bench_forty, bench_adversarial);
criterion_main!(benches);
