use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::MemFs;
use crate::fs::{FileIdentity, Mtime};

impl MemFs {
    /// Creates every parent directory. Reuses the identity if the path
    /// already exists, so writing to a file does not change what it is.
    pub fn insert_file(&self, path: impl Into<PathBuf>, text: impl Into<String>) {
        self.insert(&path.into(), text.into().into_bytes(), None);
    }

    /// As `insert_file`, with bytes that need not be UTF-8.
    pub fn insert_bytes(&self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) {
        self.insert(&path.into(), bytes.into(), None);
    }

    /// As `insert_file`, under a given identity: a real filesystem reuses a freed inode.
    pub fn insert_file_with_identity(
        &self,
        path: impl Into<PathBuf>,
        text: impl Into<String>,
        identity: FileIdentity,
    ) {
        self.insert(&path.into(), text.into().into_bytes(), Some(identity));
    }

    /// As `insert_file`, at a given mtime: a real disk's carries nanoseconds, a tick's does not.
    pub fn insert_with_mtime(&self, path: impl Into<PathBuf>, text: &str, mtime: Mtime) {
        let path = path.into();
        let mut state = self.state();
        Self::put_file(&mut state, path.clone(), text.as_bytes().to_vec(), None);
        if let Some(file) = state.files.get_mut(&path) {
            file.1 = mtime;
        }
        state.changed(&path);
    }

    fn insert(&self, path: &Path, bytes: Vec<u8>, identity: Option<FileIdentity>) {
        let mut state = self.state();
        Self::put_file(&mut state, path.to_path_buf(), bytes, identity);
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

    /// # Panics
    /// If the trashed bytes are not UTF-8.
    #[must_use]
    pub fn trashed(&self, path: &Path) -> Option<String> {
        self.state()
            .trashed
            .get(path)
            .map(|(bytes, _, _)| utf8(path, bytes))
    }

    /// How many loads of `path` have started.
    #[must_use]
    pub fn loads(&self, path: &Path) -> usize {
        self.state().loads.get(path).copied().unwrap_or_default()
    }

    /// How many byte reads of `path` have started.
    #[must_use]
    pub fn reads(&self, path: &Path) -> usize {
        self.state().reads.get(path).copied().unwrap_or_default()
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

    /// # Panics
    /// If the bytes at `path` are not UTF-8.
    #[must_use]
    pub fn read_to_string(&self, path: &Path) -> Option<String> {
        self.state()
            .files
            .get(path)
            .map(|(bytes, _, _)| utf8(path, bytes))
    }
}

fn utf8(path: &Path, bytes: &[u8]) -> String {
    let Ok(text) = String::from_utf8(bytes.to_vec()) else {
        panic!("{} is not valid UTF-8", path.display());
    };
    text
}
