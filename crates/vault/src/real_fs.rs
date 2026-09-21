//! Blocking `std::fs` inside the future, deliberately not `smol::fs`: that
//! would move a blocking read onto a second thread pool when callers already
//! run under gpui's `cx.background_spawn`, adding six crates for no
//! performance. Zed does use it (`vendor/zed-reference/crates/fs/src/fs.rs:738`).
//!
//! Unix-only: `FileIdentity` needs `dev`/`ino`, exposed only by the unix
//! `MetadataExt`. Windows has an equivalent pair, so porting is a second
//! `identity_of`, not a redesign.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use async_trait::async_trait;

use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, VaultError, normalize_line_endings,
};

pub struct RealFs;

fn io_error(path: &Path, source: std::io::Error) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn identity_of(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

fn mtime_of(path: &Path, metadata: &fs::Metadata) -> Result<Mtime> {
    metadata
        .modified()
        .map(Mtime)
        .map_err(|source| io_error(path, source))
}

#[async_trait]
impl Fs for RealFs {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let reader = fs::read_dir(path).map_err(|source| io_error(path, source))?;

        let mut entries = Vec::new();
        for entry in reader {
            let entry = entry.map_err(|source| io_error(path, source))?;
            let entry_path = entry.path();

            // Dotfiles and dot-directories never belong to a vault.
            let hidden = entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if hidden {
                continue;
            }

            // `symlink_metadata` does not follow the link: no loop detection.
            let metadata = fs::symlink_metadata(&entry_path)
                .map_err(|source| io_error(&entry_path, source))?;
            if metadata.is_symlink() {
                continue;
            }

            let kind = if metadata.is_dir() {
                Kind::Dir
            } else if entry_path.extension().is_some_and(|ext| ext == "md") {
                Kind::Note
            } else {
                continue;
            };

            entries.push(DirEntry {
                mtime: mtime_of(&entry_path, &metadata)?,
                identity: identity_of(&metadata),
                path: entry_path,
                kind,
            });
        }
        Ok(entries)
    }

    async fn load(&self, path: &Path) -> Result<Loaded> {
        // Stat before reading. The interleaving is unavoidable without locking,
        // but the order decides which way it falls: stat-first errs toward
        // "changed on disk", read-first reads as "unchanged" and silently
        // overwrites the other writer on the next save.
        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
        let text = String::from_utf8(bytes).map_err(|_| VaultError::NotUtf8 {
            path: path.to_path_buf(),
        })?;

        Ok(Loaded {
            text: normalize_line_endings(&text),
            mtime: mtime_of(path, &metadata)?,
            identity: identity_of(&metadata),
        })
    }

    async fn save(&self, path: &Path, text: &str) -> Result<Mtime> {
        fs::write(path, text).map_err(|source| io_error(path, source))?;
        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        mtime_of(path, &metadata)
    }
}
