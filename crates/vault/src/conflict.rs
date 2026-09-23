//! Where the bytes a write would have destroyed go instead.

use std::path::{Path, PathBuf};

use crate::civil::format_utc;
use crate::fs::Mtime;

/// `Note.md` becomes `Note (conflict 2026-09-21 14-32-05Z).md`, beside the
/// note. Named for the preserved file's mtime, not the current time, so the
/// name is deterministic and `vault` never reads a clock.
pub(crate) fn conflict_path(
    note: &Path,
    preserved: Mtime,
    taken: impl Fn(&Path) -> bool,
) -> PathBuf {
    let stem = note
        .file_stem()
        .unwrap_or(note.as_os_str())
        .to_string_lossy()
        .into_owned();
    let when = format_utc(preserved.0);

    let mut counter = 1;
    loop {
        let suffix = if counter == 1 {
            String::new()
        } else {
            format!(" {counter}")
        };
        let candidate = note.with_file_name(format!("{stem} (conflict {when}Z){suffix}.md"));
        if !taken(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests;
