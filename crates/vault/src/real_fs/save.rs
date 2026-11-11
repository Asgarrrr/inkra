use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{identity_of, io_error, mtime_of, stamp_of};
use crate::conflict::conflict_path;
use crate::fs::{FileIdentity, Result, Stamp};
use crate::temp::temp_path;

/// `save`'s write after the conflict check, and the stamp and identity it left.
pub(super) fn overwrite(
    path: &Path,
    text: &str,
    existing: Option<&fs::Metadata>,
) -> Result<(Stamp, FileIdentity)> {
    // A hard-linked note is written in place: replacing it would give
    // this path a new inode and strand the other link on stale text. A
    // symlink is written through for the same reason, targeting the file
    // it points at rather than replacing the link itself.
    let in_place = existing.is_some_and(|metadata| metadata.nlink() > 1)
        || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
    if in_place {
        fs::write(path, text).map_err(|source| io_error(path, source))?;
    } else {
        replace(path, text.as_bytes(), existing)?;
    }

    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    Ok((stamp_of(path, &metadata)?, identity_of(&metadata)))
}

/// Copies the bytes currently at `path` to a sibling conflict file.
///
/// Read and write rather than `fs::copy`, for the `sync_all`: these bytes
/// must survive a crash during the replace that follows. Copied raw, never
/// through `normalize_line_endings` — a conflict file is an exact record.
pub(super) fn preserve(path: &Path, metadata: &fs::Metadata) -> Result<PathBuf> {
    let destination = conflict_path(path, mtime_of(path, metadata)?, |candidate| {
        candidate.exists()
    });

    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    // `?` unlinks nothing: without this guard, a failure mid-`write_all`
    // leaves a truncated file whose name still says "preserved".
    let mut guard = TempFile(Some(destination.clone()));

    let mut file =
        fs::File::create(&destination).map_err(|source| io_error(&destination, source))?;
    file.write_all(&bytes)
        .map_err(|source| io_error(&destination, source))?;
    file.sync_all()
        .map_err(|source| io_error(&destination, source))?;
    drop(file);

    // A fresh file carries the process umask, not the note's mode.
    fs::set_permissions(&destination, metadata.permissions())
        .map_err(|source| io_error(&destination, source))?;

    guard.disarm();
    Ok(destination)
}

/// Writes `bytes` to a sibling temp file, then renames it over `path`. The
/// rename is what makes the write atomic. A sibling, not the system temp
/// dir, since a cross-device rename fails with `EXDEV`.
pub(super) fn replace(path: &Path, bytes: &[u8], existing: Option<&fs::Metadata>) -> Result<()> {
    let temp = temp_path(path);
    // `?` unlinks nothing; disarmed only once the rename succeeds.
    let mut guard = TempFile(Some(temp.clone()));

    let mut file = fs::File::create(&temp).map_err(|source| io_error(&temp, source))?;
    // Held through the rename, so `sweep` never takes a live temp for an orphan.
    let _ = file.try_lock();
    file.write_all(bytes)
        .map_err(|source| io_error(&temp, source))?;
    // Bytes reach disk before the name points at them.
    file.sync_all().map_err(|source| io_error(&temp, source))?;

    // A fresh file carries the process umask, not the note's mode.
    if let Some(metadata) = existing {
        fs::set_permissions(&temp, metadata.permissions())
            .map_err(|source| io_error(&temp, source))?;
    }

    fs::rename(&temp, path).map_err(|source| io_error(path, source))?;
    drop(file);
    guard.disarm();
    Ok(())
}

/// Removes the temp file unless the rename claimed it.
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
