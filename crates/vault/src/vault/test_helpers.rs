use std::collections::HashSet;
use std::path::Path;

use futures::executor::block_on;

use super::{EntryId, Vault};
use crate::RelPath;
use crate::mem_fs::MemFs;

pub(super) fn scan(fs: &MemFs) -> Vault {
    block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
}

pub(super) fn rel(path: &str) -> RelPath {
    RelPath::new(Path::new(path)).expect("a relative path")
}

pub(super) fn id_of(vault: &Vault, path: &str) -> Option<EntryId> {
    vault
        .entries()
        .find(|e| e.path.as_path() == Path::new(path))
        .map(|e| e.id)
}

pub(super) fn paths(vault: &Vault) -> Vec<String> {
    vault
        .entries()
        .map(|e| e.path.as_path().to_string_lossy().into_owned())
        .collect()
}

/// One row per id, and a live id answers to one inode at most (watcher spec, F5).
pub(super) fn assert_consistent(vault: &Vault) {
    let state = &vault.state;
    assert_eq!(state.by_id.len(), state.entries.len(), "one row per id");
    for (at, entry) in state.entries.iter().enumerate() {
        assert_eq!(state.by_id.get(&entry.id), Some(&at), "{entry:?}");
    }
    let mut answered = HashSet::new();
    for id in state.by_identity.values() {
        assert!(
            !state.by_id.contains_key(id) || answered.insert(*id),
            "{id:?} answers to two inodes"
        );
    }
}
