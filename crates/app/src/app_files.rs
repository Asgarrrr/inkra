//! Where the app keeps a vault's own files, outside the vault (restore-tabs spec, R1; persisted index spec, P4).

use std::path::{Path, PathBuf};

/// FNV-1a 64, written out: `DefaultHasher`'s output may change between Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// Where the session of the vault at `root`, absolute, lives under `home` (R1).
pub(crate) fn session_file(home: &Path, root: &Path) -> Option<PathBuf> {
    app_file(
        home,
        root,
        "Library/Application Support/Inkra/sessions",
        "json",
    )
}

/// Where the index cache of the vault at `root` lives under `home` (P4).
pub(crate) fn cache_file(home: &Path, root: &Path) -> Option<PathBuf> {
    app_file(home, root, "Library/Caches/Inkra/index", "idx")
}

/// Where the last vault opened is remembered (open-a-vault spec, D4).
pub(crate) fn last_vault_file(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Inkra/last-vault.json")
}

/// `None` when the file would lie inside the vault: sync clients would copy it and the watcher would see it.
fn app_file(home: &Path, root: &Path, folder: &str, extension: &str) -> Option<PathBuf> {
    let key = fnv1a(root.as_os_str().as_encoded_bytes());
    let file = home.join(folder).join(format!("{key:016x}.{extension}"));
    (!file.starts_with(root)).then_some(file)
}

#[cfg(test)]
mod tests;
