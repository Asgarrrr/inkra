use std::fmt::Write as _;
use std::hint::black_box;
use std::path::Path;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::{NoteIndex, NoteSet, Source};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Vault};

const NOTES: usize = 10_000;
const FOLDERS: usize = 100;
const LINKS_PER_NOTE: usize = 20;
/// Folder notes named `index`, the skewed stem a real vault has.
const INDEX_NOTES: usize = 500;

fn build_vault() -> Vault {
    let fs = MemFs::new();
    for note in 0..NOTES {
        let name = if note < INDEX_NOTES {
            "index".to_owned()
        } else {
            format!("note {note}")
        };
        fs.insert_file(format!("/vault/folder {}/{name}.md", note % FOLDERS), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new("/vault"))) else {
        panic!("a MemFs scan cannot fail");
    };
    vault
}

/// Every fifth link names `index`; the rest name a note by name, by path or relatively.
fn note_text(note: usize) -> String {
    let mut text = String::new();
    for link in 0..LINKS_PER_NOTE {
        let other = (note * 31 + link * 97) % NOTES;
        let _ = match link % 5 {
            0 => writeln!(text, "See [[index]] for the folder."),
            1 => writeln!(text, "Linked [[note {other}]] in passing."),
            2 => writeln!(text, "A path [[folder {}/note {other}]].", other % FOLDERS),
            3 => writeln!(
                text,
                "Relative [x](../folder%20{}/note%20{other}.md).",
                other % FOLDERS
            ),
            _ => writeln!(text, "Aliased [[note {other}|that one]] here."),
        };
    }
    text
}

/// `vault`'s entries as `NoteSet::build` takes them.
fn entries(vault: &Vault) -> Vec<(EntryId, String, Kind)> {
    vault
        .entries()
        .map(|entry| {
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (entry.id, path, entry.kind)
        })
        .collect()
}

fn built_index(vault: &Vault) -> NoteIndex {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(entries(vault)));
    let notes: Vec<EntryId> = vault
        .entries()
        .filter(|entry| entry.kind == Kind::Note)
        .map(|entry| entry.id)
        .collect();
    for (at, id) in notes.into_iter().enumerate() {
        index.insert(id, Source::parse(&note_text(at)));
    }
    index
}

fn note_named(vault: &Vault, path: &str) -> EntryId {
    let Some(entry) = vault
        .entries()
        .find(|entry| entry.path.as_path() == Path::new(path))
    else {
        panic!("no note at {path}");
    };
    entry.id
}

fn bench_set_notes(c: &mut Criterion) {
    let vault = build_vault();
    let all = entries(&vault);
    let mut index = built_index(&vault);
    c.bench_function("set_notes/10k notes, 200k links", |b| {
        b.iter_batched(
            || NoteSet::build(all.clone()),
            |notes| index.set_notes(notes),
            BatchSize::LargeInput,
        );
    });
}

fn bench_backlinks(c: &mut Criterion) {
    let vault = build_vault();
    let index = built_index(&vault);
    let ordinary = note_named(&vault, "folder 17/note 1017.md");
    let common = note_named(&vault, "folder 3/index.md");
    c.bench_function("backlinks/an ordinary note", |b| {
        b.iter(|| black_box(index.backlinks(black_box(ordinary))));
    });
    c.bench_function("backlinks/one of 500 notes named index", |b| {
        b.iter(|| black_box(index.backlinks(black_box(common))));
    });
}

fn bench_parse(c: &mut Criterion) {
    let mut text = String::new();
    while text.len() < 50_000 {
        text.push_str("Plain prose around the links, **bold** and `code`.\n");
        text.push_str(&note_text(text.len()));
    }
    c.bench_function("Source::parse/50 KB note", |b| {
        b.iter(|| black_box(Source::parse(black_box(&text))));
    });
}

criterion_group!(benches, bench_set_notes, bench_backlinks, bench_parse);
criterion_main!(benches);
