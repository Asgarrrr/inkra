use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::MemFs;
use crate::fs::FileIdentity;

impl MemFs {
    /// Creates every parent directory. Reuses the identity if the path
    /// already exists, so writing to a file does not change what it is.
    pub fn insert_file(&self, path: impl Into<PathBuf>, text: impl Into<String>) {
        self.insert(&path.into(), text.into(), None);
    }

    /// As `insert_file`, under a given identity: a real filesystem reuses a freed inode.
    pub fn insert_file_with_identity(
        &self,
        path: impl Into<PathBuf>,
        text: impl Into<String>,
        identity: FileIdentity,
    ) {
        self.insert(&path.into(), text.into(), Some(identity));
    }

    fn insert(&self, path: &Path, text: String, identity: Option<FileIdentity>) {
        let mut state = self.state();
        Self::put_file(&mut state, path.to_path_buf(), text, identity);
        state.changed(path);
    }

    /// Creates every parent directory, as `insert_file` does.
    pub fn insert_dir(&self, path: impl Into<PathBuf>) {
        let path = path.into();
        let mut state = self.state();
        Self::insert_parents(&mut state, &path);
        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        state.dirs.insert(path.clone(), (mtime, identity));
        state.changed(&path);
    }

    /// As `rm -r`: a folder goes with its subtree.
    pub fn remove(&self, path: &Path) {
        let mut state = self.state();
        let files = state.files.keys().filter(|file| file.starts_with(path));
        let dirs = state.dirs.keys().filter(|dir| dir.starts_with(path));
        let removed: BTreeSet<PathBuf> = files.chain(dirs).cloned().collect();
        for gone in &removed {
            state.files.remove(gone);
            state.dirs.remove(gone);
            state.changed(gone);
        }
    }

    pub fn hard_link(&self, from: &Path, to: impl Into<PathBuf>) {
        let mut state = self.state();
        if let Some(entry) = state.files.get(from).cloned() {
            let to = to.into();
            state.files.insert(to.clone(), entry);
            state.changed(&to);
        }
    }

    #[must_use]
    pub fn trashed(&self, path: &Path) -> Option<String> {
        self.state()
            .trashed
            .get(path)
            .map(|(text, _, _)| text.clone())
    }

    /// How many loads of `path` have started.
    #[must_use]
    pub fn loads(&self, path: &Path) -> usize {
        self.state().loads.get(path).copied().unwrap_or_default()
    }

    /// How many listings of `path` have started.
    #[must_use]
    pub fn read_dirs(&self, path: &Path) -> usize {
        self.state()
            .read_dirs
            .get(path)
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn read_to_string(&self, path: &Path) -> Option<String> {
        self.state()
            .files
            .get(path)
            .map(|(text, _, _)| text.clone())
    }
}
