use std::path::Path;

use futures::executor::block_on;
use inkra_markdown::links;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind, Vault};

use crate::note_set::prepare;
use crate::notes::NoteTable;
use crate::{NoteSet, Target};

pub(crate) const ROOT: &str = "/vault";

/// A scanned vault holding `files`, each relative to the root.
pub(crate) fn vault(files: &[&str]) -> (MemFs, Vault) {
    let fs = MemFs::new();
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
