use std::hint::black_box;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::AtomicBool;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{NameQuery, NamesSnapshot, NoteSet, match_names};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Vault};

const NOTES: usize = 50_000;
const LIMIT: usize = 100;
/// Built once: a `MemFs` scan of this vault takes minutes.
static ENTRIES: LazyLock<Vec<(EntryId, String, Kind)>> = LazyLock::new(entries);
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
const WORDS: &[&str] = &[
    "projet",
    "réunion",
    "été",
    "notes",
    "plan",
    "journal",
    "idée",
    "café",
    "budget",
    "équipe",
    "archive",
    "lecture",
    "voyage",
    "santé",
    "recherche",
    "daily",
    "meeting",
    "design",
    "draft",
    "ideas",
    "review",
    "roadmap",
    "inbox",
    "reference",
];

/// A fixed LCG, so every run benches the same vault.
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

fn entries() -> Vec<(EntryId, String, Kind)> {
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
            )
        })
        .collect()
}

/// Generated notes without a scan: matching reads only the paths, and a `MemFs` scan of
/// these trees would take far longer than the bench.
fn generated(path: impl Fn(&mut Lcg, usize) -> String) -> Vec<(EntryId, String, Kind)> {
    let mut lcg = Lcg(11);
    (0..NOTES)
        .map(|note| (EntryId::DETACHED, path(&mut lcg, note), Kind::Note))
        .collect()
}

fn folders(lcg: &mut Lcg, depth: usize) -> String {
    (0..depth)
        .map(|_| WORDS[lcg.next(WORDS.len())])
        .collect::<Vec<_>>()
        .join("/")
}

/// About 56-char paths: two or three folders, then "Meeting with Zoé about the roadmap 12".
fn long_stems() -> Vec<(EntryId, String, Kind)> {
    generated(|lcg, note| {
        let depth = 2 + lcg.next(2);
        let folders = folders(lcg, depth);
        let kind = KINDS[lcg.next(KINDS.len())];
        let person = PEOPLE[lcg.next(PEOPLE.len())];
        let topic = TOPICS[lcg.next(TOPICS.len())];
        format!("{folders}/{kind} with {person} about {topic} {note}.md")
    })
}

/// 7 to 9 components: 6 to 8 folders, then a stem `<word> <n>`.
fn deep() -> Vec<(EntryId, String, Kind)> {
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
        .filter(|(_, _, kind)| *kind == Kind::Note)
        .step_by(2_500)
        .map(|(id, _, _)| *id)
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

criterion_group!(benches, bench_build, bench_match, bench_realistic);
criterion_main!(benches);
