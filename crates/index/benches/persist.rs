use std::hint::black_box;
use std::path::Path;
use std::time::{Instant, UNIX_EPOCH};

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::persist::{decode, encode};
use inkra_index::{FileRead, MergeWhen, NoteContent, NoteIndex, NoteSet};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{FileIdentity, Kind, Mtime, Stamp, Vault};

const ROOT: &str = "/vault";
const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
const WORDS: &[&str] = &[
    "le", "la", "les", "de", "des", "et", "un", "une", "projet", "projets", "réunion", "été",
    "équipe", "idée", "café", "santé", "déjà", "très", "être", "après", "the", "and", "of", "to",
    "project", "meeting", "summer", "team", "idea", "coffee", "notes", "plan", "draft", "review",
    "roadmap", "budget", "voyage", "lecture", "journal", "archive",
];

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

/// 1 to 5 KB of prose, lines of 8 to 20 words, and one link to another note.
fn note_text(lcg: &mut Lcg) -> String {
    let target = 1_024 + lcg.next(4_097);
    let mut text = format!("[[n{}]]\n", lcg.next(NOTES));
    while text.len() < target {
        for word in 0..8 + lcg.next(13) {
            if word > 0 {
                text.push(if lcg.next(9) == 0 { ',' } else { ' ' });
            }
            text.push_str(WORDS[lcg.next(WORDS.len())]);
        }
        text.push_str(".\n");
    }
    text
}

/// Every note loaded at a distinct stamp and identity, its words merged.
fn index() -> NoteIndex {
    let fs = MemFs::new();
    for note in 0..NOTES {
        fs.insert_file(format!("{ROOT}/f{}/n{note}.md", note % FOLDERS), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new(ROOT))) else {
        panic!("a MemFs scan cannot fail");
    };
    let entries: Vec<_> = vault
        .entries()
        .map(|entry| {
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (entry.id, path, entry.kind, entry.mtime)
        })
        .collect();
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries.clone()));
    let mut lcg = Lcg(13);
    let notes = entries.iter().filter(|(_, _, kind, _)| *kind == Kind::Note);
    for (inode, (id, ..)) in (0..).zip(notes) {
        let text = note_text(&mut lcg);
        let read = FileRead {
            stamp: Stamp {
                mtime: Mtime(UNIX_EPOCH),
                size: text.len() as u64,
            },
            identity: FileIdentity { device: 1, inode },
        };
        index.insert(*id, NoteContent::parse(&text, read));
    }
    while let Some(merge) = index.words_merge(MergeWhen::Settle) {
        let _ = index.words_apply(merge.run());
    }
    index
}

fn bench_persist(c: &mut Criterion) {
    let start = Instant::now();
    let index = index();
    eprintln!("index of {NOTES} notes: {:?}", start.elapsed());
    let root = Path::new(ROOT);
    let snapshot = index.snapshot();
    let Some(bytes) = encode(&snapshot, root) else {
        panic!("the bench index is under the cap");
    };
    eprintln!("cache file: {} bytes", bytes.len());
    assert!(decode(&bytes, root).is_ok(), "the bench file decodes");
    let mut group = c.benchmark_group("persist");
    group.sample_size(10);
    group.bench_function("encode", |b| b.iter(|| black_box(encode(&snapshot, root))));
    group.bench_function("decode", |b| b.iter(|| black_box(decode(&bytes, root))));
    group.finish();
}

criterion_group!(benches, bench_persist);
criterion_main!(benches);
