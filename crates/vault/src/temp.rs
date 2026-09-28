//! The temporary file a save writes before renaming it onto the note.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TEMP_MARK: &str = ".inkra-";
/// Not `.md`, so `read_dir`'s filter hides a file left by a crash.
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
