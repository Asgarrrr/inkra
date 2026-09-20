//! The platform filesystem.
//!
//! Every method performs blocking `std::fs` work inside the future, and does
//! not use `smol`. `smol::fs` sits on the `blocking` crate's thread pool, so
//! on local disk it only moves a blocking read onto a different pool — and our
//! callers already run under gpui's `cx.background_spawn`, which is one.
//! Copying Zed that far would add six crates and buy no performance; Zed's own
//! `RealFs` is written against `smol::fs`
//! (`vendor/zed-reference/crates/fs/src/fs.rs:738`).
//!
//! Unix-only: `FileIdentity` needs `dev`/`ino`, which only the unix
//! `MetadataExt` exposes. Windows has an equivalent pair behind a different
//! API, so porting is a second `identity_of`, not a redesign.

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

            // `symlink_metadata` does not follow the link, which is how the
            // scan avoids symlink loops without needing to detect them.
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
        // Stat before reading, not after. A write landing between the two
        // calls is unavoidable without locking, but the order decides which
        // way the race falls: stat-first pairs new text with an old mtime, so
        // a later conflict check errs toward "changed on disk". Read-first
        // pairs stale text with a fresh mtime, which reads as "unchanged" and
        // silently overwrites the other writer on the next save.
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
