//! Blocking `std::fs` inside the future: callers already run under gpui's
//! `cx.background_spawn`, so a second thread pool buys nothing.
//!
//! Unix-only: `FileIdentity` needs `dev`/`ino`, from the unix `MetadataExt`.

use std::fs;
use std::io;
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

            let hidden = entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if hidden {
                continue;
            }

            // Does not follow the link, so a symlink loop cannot recurse here.
            let metadata = fs::symlink_metadata(&entry_path)
                .map_err(|source| io_error(&entry_path, source))?;
            if metadata.is_symlink() {
                continue;
            }

            let kind = if metadata.is_dir() {
                Kind::Dir
            } else if entry_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
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
        // Stat before reading: on the racing case, this errs toward "changed
        // on disk" rather than silently overwriting on the next save.
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
        // Only ENOENT means "not there"; other errors (ESTALE, EIO, EACCES on
        // a network mount) describe a file we can't safely rename over.
        let existing = match fs::metadata(path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(path, error)),
        };

        // A missing file is not a conflict: no bytes to preserve.
        let preserved = match existing.as_ref() {
            Some(metadata) if stamp_of(path, metadata)? != expected => {
                Some(preserve(path, metadata)?)
            }
            _ => None,
        };

        // A hard-linked note is written in place: replacing it would give
        // this path a new inode and strand the other link on stale text. A
        // symlink is written through for the same reason, targeting the file
        // it points at rather than replacing the link itself.
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

    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity> {
        match kind {
            // O_EXCL: the kernel refuses an existing path atomically, closing
            // the window an `exists()` check plus `File::create` would leave.
            Kind::Note => fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(drop),
            Kind::Dir => fs::create_dir(path),
        }
        .map_err(|source| io_error(path, source))?;

        let metadata = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
        Ok(identity_of(&metadata))
    }

    /// `rename(2)` refuses every collision except one: a note renamed over an
    /// existing note is silently removed first. `hard_link` closes that gap,
    /// since `link(2)` refuses an existing destination atomically.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        // A symlink renames as itself; `hard_link` would resolve it and leave
        // a plain link to the target instead.
        if fs::symlink_metadata(from).is_ok_and(|metadata| metadata.is_symlink()) {
            return guarded_rename(from, to);
        }

        match fs::hard_link(from, to) {
            // Both names now share one inode; dropping the old one leaves the
            // file at `to` with its identity, and so its `EntryId`, untouched.
            Ok(()) => fs::remove_file(from).map_err(|source| io_error(from, source)),

            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // On a case-insensitive or normalization-insensitive volume,
                // `to` may *be* `from` under another spelling. Asked by
                // inode, since that's a property of the volume, not the names.
                if same_file(from, to) {
                    return fs::rename(from, to).map_err(|source| io_error(to, source));
                }
                Err(io_error(to, error))
            }

            // No hard links on this volume (a directory, exFAT, some network
            // mounts): fall back to the stat-then-rename window `save` also
            // accepts.
            Err(_) => guarded_rename(from, to),
        }
    }

    async fn trash(&self, path: &Path) -> Result<()> {
        let mut context = trash::TrashContext::default();
        trash_method(&mut context);
        // Never `canonicalize`: resolving symlinks would trash the link's
        // target and leave the link itself behind.
        let absolute = std::path::absolute(path).map_err(|source| io_error(path, source))?;
        context.delete(&absolute).map_err(|source| VaultError::Io {
            path: absolute,
            source: io::Error::other(source),
        })
    }
}

/// The crate's macOS default shells out to `osascript`, raising an Automation
/// permission prompt; `NsFileManager` calls `trashItemAtURL` directly instead,
/// at the cost of Finder's "Put Back" metadata.
#[cfg(target_os = "macos")]
fn trash_method(context: &mut trash::TrashContext) {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};

    context.set_delete_method(DeleteMethod::NsFileManager);
}

#[cfg(not(target_os = "macos"))]
fn trash_method(_context: &mut trash::TrashContext) {}

/// Whether two paths name one file — what a case-insensitive volume needs
/// answered and string comparison cannot.
fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(a), Ok(b)) = (fs::symlink_metadata(a), fs::symlink_metadata(b)) else {
        return false;
    };
    identity_of(&a) == identity_of(&b)
}

/// `fs::rename`, refusing a destination that is a different file. The stat
/// and the rename are not atomic, so a file created between them is still
/// clobbered — the same window `Fs::save` already accepts.
fn guarded_rename(from: &Path, to: &Path) -> Result<()> {
    // Checked first so a missing source reports as missing at `from`, not as
    // a collision at `to`.
    if let Err(source) = fs::symlink_metadata(from) {
        return Err(io_error(from, source));
    }
    if fs::symlink_metadata(to).is_ok() && !same_file(from, to) {
        return Err(io_error(to, io::Error::from(io::ErrorKind::AlreadyExists)));
    }
    fs::rename(from, to).map_err(|source| io_error(to, source))
}

/// Copies the bytes currently at `path` to a sibling conflict file.
///
/// Read and write rather than `fs::copy`, for the `sync_all`: these bytes
/// must survive a crash during the replace that follows. Copied raw, never
/// through `normalize_line_endings` — a conflict file is an exact record.
fn preserve(path: &Path, metadata: &fs::Metadata) -> Result<PathBuf> {
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

/// Writes `text` to a sibling temp file, then renames it over `path`. The
/// rename is what makes the write atomic. A sibling, not the system temp
/// dir, since a cross-device rename fails with `EXDEV`.
fn replace(path: &Path, text: &str, existing: Option<&fs::Metadata>) -> Result<()> {
    let temp = temp_path(path);
    // `?` unlinks nothing; disarmed only once the rename succeeds.
    let mut guard = TempFile(Some(temp.clone()));

    let mut file = fs::File::create(&temp).map_err(|source| io_error(&temp, source))?;
    file.write_all(text.as_bytes())
        .map_err(|source| io_error(&temp, source))?;
    // Bytes reach disk before the name points at them.
    file.sync_all().map_err(|source| io_error(&temp, source))?;
    drop(file);

    // A fresh file carries the process umask, not the note's mode.
    if let Some(metadata) = existing {
        fs::set_permissions(&temp, metadata.permissions())
            .map_err(|source| io_error(&temp, source))?;
    }

    fs::rename(&temp, path).map_err(|source| io_error(path, source))?;
    guard.disarm();
    Ok(())
}

/// Unique per process and per call, so two concurrent saves can't collide.
fn temp_path(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let counter = NEXT.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    // `.tmp`, so `read_dir`'s `*.md` filter hides a file left by a crash.
    name.push(format!(".inkra-{pid}-{counter}.tmp"));
    path.with_file_name(name)
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
