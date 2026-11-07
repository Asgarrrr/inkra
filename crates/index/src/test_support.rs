use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use futures::executor::block_on;
use inkra_markdown::links;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, FileIdentity, Fs, Kind, Mtime, Serial, Stamp, Vault};

use crate::{
    FileRead, IndexSnapshot, NoteContent, NoteIndex, NoteMatch, NoteQuery, NoteSet, Read, Searched,
    Target, TextSearch,
};

pub(crate) const ROOT: &str = "/vault";

/// The oracles' alphabet: S1's edges (marks, case orbits, Hangul, apostrophes, long words).
pub(crate) const PARTS: &[&str] = &[
    "a",
    "e",
    "é",
    "e\u{301}",
    "É",
    "E",
    "ß",
    "ss",
    "\u{212A}",
    "k",
    "İ",
    "i",
    "ı",
    "한",
    "\u{1112}",
    "'",
    "’",
    "-",
    "_",
    "1",
    "🎉",
    "\u{301}",
    " ",
    "\n",
    ".",
    "l'été",
    "aaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "ς",
    "σ",
    "Σ",
    "ſ",
    "s",
    "\u{fe0f}\u{20e3}",
    "\u{200d}",
    "किताब",
    "คืน",
    "كَتَبَ",
    "\u{34F}",
    "\u{94D}",
    "\u{93C}",
];

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
pub(crate) fn entries(vault: &Vault) -> Vec<(EntryId, String, Kind, Mtime)> {
    vault
        .entries()
        .map(|entry| {
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (entry.id, path, entry.kind, entry.mtime)
        })
        .collect()
}

/// `n` seconds after the epoch; `MemFs` stamps its files a million seconds later.
pub(crate) fn mtime(n: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(n))
}

/// A stamp at `mtime(n)` of an empty file.
pub(crate) fn stamp(n: u64) -> Stamp {
    Stamp {
        mtime: mtime(n),
        size: 0,
    }
}

/// `vault`'s notes as `NoteIndex::set_notes` takes them.
pub(crate) fn note_set(vault: &Vault) -> NoteSet {
    NoteSet::build(entries(vault))
}

/// An index of `vault`'s files with no text loaded.
pub(crate) fn resolver(vault: &Vault) -> NoteIndex {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(vault));
    index
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

/// `text` parsed as a load at `mtime` gives it, its size the text's length.
pub(crate) fn content(text: &str, mtime: Mtime) -> NoteContent {
    let size = text.len() as u64;
    let identity = FileIdentity {
        device: 1,
        inode: 1,
    };
    NoteContent::parse(
        text,
        FileRead {
            stamp: Stamp { mtime, size },
            identity,
        },
    )
}

/// `id`'s links, stamp and words, as a load gives them, stamped `mtime(0)`.
pub(crate) fn load(index: &mut NoteIndex, id: EntryId, text: &str) {
    index.insert(id, content(text, mtime(0)));
}

/// Each loaded note of `snapshot` with its stamp, in vault order.
pub(crate) fn loaded(snapshot: &IndexSnapshot) -> Vec<(EntryId, Stamp)> {
    (0..snapshot.names().len())
        .filter_map(|order| snapshot.candidate(order))
        .map(|candidate| (candidate.id, candidate.stamp))
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

/// `find_notes` over the whole vault, on one job.
pub(crate) fn find_all(
    snapshot: &IndexSnapshot,
    query: &NoteQuery,
    limit: usize,
) -> Vec<NoteMatch> {
    let found = block_on(snapshot.find_notes(query, limit, &Serial, &AtomicBool::new(false)));
    let Some(found) = found else {
        panic!("not cancelled");
    };
    found
}

/// Each loaded note of `texts` by its id in `ids`, as `search_texts` reads them.
pub(crate) fn text_map(ids: &[EntryId], texts: &[Option<&str>]) -> HashMap<EntryId, Arc<str>> {
    ids.iter()
        .zip(texts)
        .filter_map(|(id, text)| text.map(|text| (*id, Arc::from(text))))
        .collect()
}

/// Vault search over `texts`, each candidate read unchanged since its load, in candidate order.
pub(crate) fn search_texts(
    snapshot: &IndexSnapshot,
    texts: &HashMap<EntryId, Arc<str>>,
    query: &NoteQuery,
    limit: usize,
) -> Searched {
    let cancel = AtomicBool::new(false);
    let Some(candidates) = snapshot.candidates(query, &cancel) else {
        panic!("not cancelled");
    };
    let mut search = TextSearch::new(query, limit);
    for candidate in &candidates {
        let Some(text) = texts.get(&candidate.id) else {
            panic!("candidate {:?} has no text", candidate.id);
        };
        let read = Read::Text {
            text: Arc::clone(text),
            stamp: candidate.stamp,
        };
        if !search.read(candidate, read, &cancel) {
            break;
        }
    }
    let Some(found) = search.finish(&cancel) else {
        panic!("not cancelled");
    };
    found.searched
}
