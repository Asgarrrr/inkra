//! Blocking `std::fs` inside the future, deliberately not `smol::fs`: that
//! would move a blocking read onto a second thread pool when callers already
//! run under gpui's `cx.background_spawn`, adding six crates for no
//! performance. Zed does use it (`vendor/zed-reference/crates/fs/src/fs.rs:738`).
//!
//! Unix-only: `FileIdentity` needs `dev`/`ino`, exposed only by the unix
//! `MetadataExt`. Windows has an equivalent pair, so porting is a second
//! `identity_of`, not a redesign.

use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;

use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, VaultError,
    normalize_line_endings,
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

    async fn save(&self, path: &Path, text: &str) -> Result<Saved> {
        let existing = fs::metadata(path).ok();

        // A note with a second hard link is written in place. Replacing it
        // would give this path a new inode and leave the other link on the old
        // one: `Vault::rescan` collapses two names sharing an identity into one
        // entry (`vault.rs:108-110`), so the pair would stop sharing and the
        // second name would surface as a new row carrying stale text — a
        // duplicate appearing because of a write nobody asked for.
        let linked = existing
            .as_ref()
            .is_some_and(|metadata| metadata.nlink() > 1);
        if linked {
            fs::write(path, text).map_err(|source| io_error(path, source))?;
        } else {
            replace(path, text, existing.as_ref())?;
        }

        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        Ok(Saved {
            mtime: mtime_of(path, &metadata)?,
            identity: identity_of(&metadata),
        })
    }
}

/// Write `text` to a sibling temp file, then rename it over `path`. The rename
/// is what makes the write atomic: at no point does `path` name a partial note.
///
/// A sibling rather than the system temp dir, for the reason Zed gives
/// (`vendor/zed-reference/crates/fs/src/fs.rs:976-978`): a rename across
/// devices fails with `EXDEV`, and nothing says the vault shares a filesystem
/// with `/tmp`.
///
/// This needs write and execute permission on the **parent directory**, which
/// the truncating write it replaces did not: `fs::write` to an existing file
/// touches no directory entry. A vault whose notes are writable inside a
/// directory that is not is the one layout this regresses.
fn replace(path: &Path, text: &str, existing: Option<&fs::Metadata>) -> Result<()> {
    let temp = temp_path(path);
    // `?` unlinks nothing on its way out, and every step below leaves a file on
    // disk behind it. Disarmed only once the rename has succeeded.
    let mut guard = TempFile(Some(temp.clone()));

    let mut file = fs::File::create(&temp).map_err(|source| io_error(&temp, source))?;
    file.write_all(text.as_bytes())
        .map_err(|source| io_error(&temp, source))?;
    // The bytes reach disk before the name points at them. This does *not* make
    // the rename itself durable — that is a directory metadata change and needs
    // the parent fsynced, which neither we nor Zed do. See the design spec.
    file.sync_all().map_err(|source| io_error(&temp, source))?;
    drop(file);

    // A fresh file carries the process umask, not the note's mode. Skipping
    // this silently narrows a 0644 note to whatever the temp file was created
    // with. Owner, group and ACLs are lost regardless — see the spec's costs.
    if let Some(metadata) = existing {
        fs::set_permissions(&temp, metadata.permissions())
            .map_err(|source| io_error(&temp, source))?;
    }

    fs::rename(&temp, path).map_err(|source| io_error(path, source))?;
    guard.disarm();
    Ok(())
}

/// Unique per process and per call: two saves of one note must not collide on
/// the temp name, and a second process saving the same vault must not either.
fn temp_path(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let counter = NEXT.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    // Extension `tmp`, so `read_dir`'s `*.md` filter hides it: a temp file that
    // outlives a crash must not appear in the vault as a note.
    name.push(format!(".inkra-{pid}-{counter}.tmp"));
    path.with_file_name(name)
}

/// Removes the temp file unless the rename claimed it. A hard kill between
/// `File::create` and the rename leaves one behind that nothing can clean up;
/// it is invisible in the vault, and its trigger is in the design spec.
struct TempFile(Option<PathBuf>);

impl TempFile {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
