use std::path::Path;

use futures::executor::block_on;

use super::Vault;
use crate::RelPath;
use crate::mem_fs::MemFs;

pub(super) fn scan(fs: &MemFs) -> Vault {
    block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
}

pub(super) fn rel(path: &str) -> RelPath {
    RelPath::new(Path::new(path)).expect("a relative path")
}

pub(super) fn paths(vault: &Vault) -> Vec<String> {
    vault
        .entries()
        .map(|e| e.path.as_path().to_string_lossy().into_owned())
        .collect()
}
