use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::State;
use super::errors::io;
use crate::fs::Result;

impl State {
    /// Moves the stored entry rather than rebuilding it, so the identity
    /// travels with the file as an inode does under `rename(2)`. The subtree
    /// moves with a directory for the same reason.
    pub(super) fn rename(&mut self, from: &Path, to: &Path) -> Result<()> {
        if self.uncreatable.contains(to) {
            return Err(io(to, ErrorKind::PermissionDenied));
        }
        // Byte equality only: unlike a real volume, this fake does not fold
        // case or Unicode normalization.
        if from == to {
            return Ok(());
        }
        if self.exists(to) {
            return Err(io(to, ErrorKind::AlreadyExists));
        }
        // As `rename(2)`: nothing moves into its own subtree.
        if to.starts_with(from) {
            if self.files.contains_key(from) {
                return Err(io(to, ErrorKind::NotADirectory));
            }
            if self.dirs.contains_key(from) {
                return Err(io(to, ErrorKind::InvalidInput));
            }
        }
        if self.missing_parent(to) {
            return Err(io(to, ErrorKind::NotFound));
        }

        if let Some(entry) = self.files.remove(from) {
            self.files.insert(to.to_path_buf(), entry);
            self.changed(from);
            self.changed(to);
            return Ok(());
        }
        let Some(entry) = self.dirs.remove(from) else {
            return Err(io(from, ErrorKind::NotFound));
        };
        self.dirs.insert(to.to_path_buf(), entry);
        move_subtree(&mut self.files, from, to);
        move_subtree(&mut self.dirs, from, to);
        // As FSEvents: the two names, not the subtree that moved with them.
        self.changed(from);
        self.changed(to);
        Ok(())
    }
}

/// Re-keys every entry under `from` to the same place under `to`.
fn move_subtree<V>(entries: &mut BTreeMap<PathBuf, V>, from: &Path, to: &Path) {
    *entries = std::mem::take(entries)
        .into_iter()
        .map(|(path, entry)| match path.strip_prefix(from) {
            Ok(relative) => (to.join(relative), entry),
            Err(_) => (path, entry),
        })
        .collect();
}
