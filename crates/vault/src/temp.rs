//! The temporary file a save writes before renaming it onto the note.

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TEMP_MARK: &str = ".inkra-";
/// `is_temp` matches it with `TEMP_MARK`, so `listable` hides a file a crash left.
const TEMP_EXTENSION: &str = ".tmp";

/// Unique per process and per call, so two concurrent saves can't collide.
pub(crate) fn temp_path(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let counter = NEXT.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!("{TEMP_MARK}{pid}-{counter}{TEMP_EXTENSION}"));
    path.with_file_name(name)
}

/// Whether `temp_path` could have made this name.
pub(crate) fn is_temp(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.ends_with(TEMP_EXTENSION) && name.contains(TEMP_MARK))
}

/// Removes the temps `temp_path` made for `path` that no process holds: a writer keeps its
/// temp locked until the rename, so a lock taken here marks an orphan of a crash.
pub(crate) fn sweep(path: &Path) {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let prefix = format!("{}{TEMP_MARK}", name.to_string_lossy());
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !(is_temp(&name) && name.to_string_lossy().starts_with(&prefix)) {
            continue;
        }
        let orphan = entry.path();
        let Ok(file) = OpenOptions::new().write(true).open(&orphan) else {
            continue;
        };
        if file.try_lock().is_ok() {
            let _ = fs::remove_file(&orphan);
        }
    }
}
