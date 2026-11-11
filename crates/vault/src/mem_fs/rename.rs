use std::io::ErrorKind;
use std::path::Path;

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
        // As `rename(2)`: nothing moves into its own subtree.
        if to.starts_with(from) {
            if self.files.contains_key(from) {
                return Err(io(to, ErrorKind::NotADirectory));
            }
            if self.dirs.contains_key(from) {
                return Err(io(to, ErrorKind::InvalidInput));
            }
        }
        if self.files.contains_key(to) || self.dirs.contains_key(to) {
            return Err(io(to, ErrorKind::AlreadyExists));
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

        let moved: Vec<_> = self
            .files
            .keys()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        for path in moved {
            let Ok(relative) = path.strip_prefix(from) else {
                continue;
            };
            let destination = to.join(relative);
            if let Some(entry) = self.files.remove(&path) {
                self.files.insert(destination, entry);
            }
        }
        let moved: Vec<_> = self
            .dirs
            .keys()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        for path in moved {
            let Ok(relative) = path.strip_prefix(from) else {
                continue;
            };
            let destination = to.join(relative);
            if let Some(entry) = self.dirs.remove(&path) {
                self.dirs.insert(destination, entry);
            }
        }
        // As FSEvents: the two names, not the subtree that moved with them.
        self.changed(from);
        self.changed(to);
        Ok(())
    }
}
