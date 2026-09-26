use std::path::Path;

use futures::executor::block_on;
use inkra_markdown::links;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind, Vault};

use crate::note_set::prepare;
use crate::notes::NoteTable;
use crate::{CorpusSnapshot, NoteContent, NoteIndex, NoteSet, Target};

pub(crate) const ROOT: &str = "/vault";

/// A scanned vault holding `files`, each relative to the root.
pub(crate) fn vault(files: &[&str]) -> (MemFs, Vault) {
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for file in files {
        fs.insert_file(format!("{ROOT}/{file}"), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new(ROOT))) else {
        panic!("a MemFs scan cannot fail");
    };
    (fs, vault)
}

/// Whether `MemFs` accepted the rename; the vault is rescanned either way.
pub(crate) fn try_rename(fs: &MemFs, vault: &mut Vault, from: &str, to: &str) -> bool {
    let (from, to) = (format!("{ROOT}/{from}"), format!("{ROOT}/{to}"));
    let renamed = block_on(fs.rename(Path::new(&from), Path::new(&to))).is_ok();
    let Ok(()) = block_on(vault.rescan(fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    renamed
}

pub(crate) fn rename(fs: &MemFs, vault: &mut Vault, from: &str, to: &str) {
    assert!(try_rename(fs, vault, from, to), "rename {from} to {to}");
}

/// `vault`'s entries as `NoteSet::build` takes them.
pub(crate) fn entries(vault: &Vault) -> Vec<(EntryId, String, Kind)> {
    vault
        .entries()
        .map(|entry| {
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (entry.id, path, entry.kind)
        })
        .collect()
}

/// `vault`'s notes as `NoteIndex::set_notes` takes them.
pub(crate) fn note_set(vault: &Vault) -> NoteSet {
    NoteSet::build(entries(vault))
}

pub(crate) fn table(vault: &Vault) -> NoteTable {
    NoteTable::new(&prepare(entries(vault)))
}

pub(crate) fn id(vault: &Vault, path: &str) -> EntryId {
    let Some(entry) = vault
        .entries()
        .find(|entry| entry.path.as_path() == Path::new(path))
    else {
        panic!("no entry at {path}");
    };
    entry.id
}

/// The one link written in `text`.
pub(crate) fn target(text: &str) -> Target {
    let found = links(text);
    assert_eq!(found.len(), 1, "one link in {text:?}");
    Target::parse(&found[0])
}

/// `id`'s links, text and words, as a load gives them.
pub(crate) fn load(index: &mut NoteIndex, id: EntryId, text: &str) {
    index.insert(id, NoteContent::parse(text.into()));
}

/// Each loaded note of `corpus` with its text, in vault order.
pub(crate) fn texts(corpus: &CorpusSnapshot) -> Vec<(EntryId, String)> {
    corpus
        .notes(0..corpus.len())
        .map(|(id, text)| (id, text.to_owned()))
        .collect()
}

/// An index of one note per text, in the given order; `None` is a note not loaded.
pub(crate) fn indexed(texts: &[Option<&str>]) -> (Vec<EntryId>, NoteIndex) {
    let files: Vec<String> = (0..texts.len()).map(|n| format!("n{n:03}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
    for (id, text) in ids.iter().zip(texts) {
        if let Some(text) = text {
            load(&mut index, *id, text);
        }
    }
    (ids, index)
}

/// A corpus of one note per text, in the given order; `None` is a note not loaded.
pub(crate) fn corpus(texts: &[Option<&str>]) -> (Vec<EntryId>, CorpusSnapshot) {
    let (ids, index) = indexed(texts);
    (ids, index.corpus())
}
