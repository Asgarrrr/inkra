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

use crate::conflict::conflict_path;
use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError,
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

fn stamp_of(path: &Path, metadata: &fs::Metadata) -> Result<Stamp> {
    Ok(Stamp {
        mtime: mtime_of(path, metadata)?,
        size: metadata.len(),
    })
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
            stamp: stamp_of(path, &metadata)?,
            identity: identity_of(&metadata),
        })
    }

    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        // `.ok()` here would read every error as "the file is not there" and
        // go straight to the replace. `ENOENT` is the only error that means
        // that; `ESTALE`, `EIO` or `EACCES` on a network or FUSE mount leave a
        // file we cannot describe, and a rename over it is the clobber this
        // whole parameter exists to prevent.
        let existing = match fs::metadata(path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(path, error)),
        };

        // Preserved *before* the replace, never after: between the two steps
        // the bytes exist twice, so no crash point loses them. The reverse
        // order makes a crash destroy the version this exists to protect.
        //
        // A missing file is not a conflict. There are no bytes to preserve, and
        // recreating the note loses nothing.
        let preserved = match existing.as_ref() {
            Some(metadata) if stamp_of(path, metadata)? != expected => {
                Some(preserve(path, metadata)?)
            }
            _ => None,
        };

        // A note with a second hard link is written in place. Replacing it
        // would give this path a new inode and leave the other link on the old
        // one: `Vault::rescan` collapses two names sharing an identity into one
        // entry (`vault.rs:108-110`), so the pair would stop sharing and the
        // second name would surface as a new row carrying stale text — a
        // duplicate appearing because of a write nobody asked for.
        //
        // A symlink is written through for the same reason: the rename would
        // replace the link with a regular file and strand its target holding
        // stale text. `read_dir` keeps symlinks out of a vault, but a file
        // named on the command line — `EntryId::DETACHED` — never passed that
        // filter. `fs::metadata` follows the link, so the `nlink` above is the
        // target's and cannot stand in for this check.
        let in_place = existing
            .as_ref()
            .is_some_and(|metadata| metadata.nlink() > 1)
            || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
        if in_place {
            fs::write(path, text).map_err(|source| io_error(path, source))?;
        } else {
            replace(path, text, existing.as_ref())?;
        }

        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        Ok(Saved {
            stamp: stamp_of(path, &metadata)?,
            identity: identity_of(&metadata),
            preserved,
        })
    }
}

/// Copies the bytes currently at `path` to a sibling conflict file, and returns
/// where they went.
///
/// Read and write rather than `fs::copy`, for the `sync_all`: the whole point
/// is that these bytes survive a crash a moment later, during the replace.
///
/// The bytes are copied raw, never through `normalize_line_endings`. A conflict
/// file is an exact record of what was on disk; rewriting its line endings
/// would make it disagree with the version it exists to preserve.
fn preserve(path: &Path, metadata: &fs::Metadata) -> Result<PathBuf> {
    let destination = conflict_path(path, mtime_of(path, metadata)?, |candidate| {
        candidate.exists()
    });

    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    // The same guard `replace` uses, for the same reason: `?` unlinks nothing.
    // Without it a failure part-way through `write_all` — `ENOSPC` is the
    // ordinary one — leaves a truncated file whose name says "preserved". The
    // retry then finds that name taken and appends ` 2`, so the user is left
    // with two conflict files, one of which is a lie.
    let mut guard = TempFile(Some(destination.clone()));

    let mut file =
        fs::File::create(&destination).map_err(|source| io_error(&destination, source))?;
    file.write_all(&bytes)
        .map_err(|source| io_error(&destination, source))?;
    file.sync_all()
        .map_err(|source| io_error(&destination, source))?;
    drop(file);

    // The same reason `replace` restores the mode onto its temp file: a fresh
    // file carries the process umask, not the note's. Skipping it re-exposes a
    // deliberately narrow note's content at whatever default the umask gives.
    fs::set_permissions(&destination, metadata.permissions())
        .map_err(|source| io_error(&destination, source))?;

    guard.disarm();
    Ok(destination)
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
