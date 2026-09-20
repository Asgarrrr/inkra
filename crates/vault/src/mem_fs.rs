//! An in-memory `Fs` for tests.
//!
//! A `BTreeMap`, an injectable error and one load gate, not a simulated
//! kernel. Zed's `FakeFs` is 2207 lines
//! (`vendor/zed-reference/crates/fs/src/fs.rs:1440-3646`) because it amortizes
//! over hundreds of scanner tests; ours does not.
//!
//! Compiled under clippy's `--all-features`, so it is held to
//! `unwrap_used = "deny"` like everything else.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use futures::channel::oneshot;

use crate::fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, VaultError};

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
    dirs: BTreeMap<PathBuf, (Mtime, FileIdentity)>,
    next_inode: u64,
    /// `read_dir` on exactly this path fails. Models an unreadable directory;
    /// its children are unreachable anyway, since the walk never descends past
    /// a directory it could not read.
    unreadable: Option<PathBuf>,
    /// The next `load` of each of these paths parks until its `LoadGate` is
    /// released or dropped. See `MemFs::block_load`.
    blocked: BTreeMap<PathBuf, oneshot::Receiver<()>>,
}

pub struct MemFs {
    state: Mutex<State>,
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemFs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                next_inode: 1,
                ..State::default()
            }),
        }
    }

    /// Recovers the guard from a poisoned lock rather than panicking. The lock
    /// is only poisoned when another test thread already panicked, and its
    /// failure is the one worth reading; `unwrap` is denied anyway.
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn fresh_identity(state: &mut State) -> FileIdentity {
        let inode = state.next_inode;
        state.next_inode += 1;
        FileIdentity { device: 1, inode }
    }

    fn now() -> Mtime {
        Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000))
    }

    /// Inserts a file, creating every parent directory. Reuses the identity if
    /// the path already exists, so writing to a file does not change what it
    /// is.
    pub fn insert_file(&self, path: impl Into<PathBuf>, text: impl Into<String>) {
        let path = path.into();
        let mut state = self.state();
        let mut parent = path.parent();
        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() || state.dirs.contains_key(dir) {
                break;
            }
            let identity = Self::fresh_identity(&mut state);
            state
                .dirs
                .insert(dir.to_path_buf(), (Self::now(), identity));
            parent = dir.parent();
        }
        let existing = state.files.get(&path).map(|(_, _, id)| *id);
        let identity = existing.unwrap_or_else(|| Self::fresh_identity(&mut state));
        state
            .files
            .insert(path, (text.into(), Self::now(), identity));
    }

    pub fn insert_dir(&self, path: impl Into<PathBuf>) {
        let path = path.into();
        let mut state = self.state();
        let identity = Self::fresh_identity(&mut state);
        state.dirs.insert(path, (Self::now(), identity));
    }

    pub fn remove(&self, path: &Path) {
        let mut state = self.state();
        state.files.remove(path);
        state.dirs.remove(path);
    }

    /// A second name for the same file: one identity, two paths. What the scan
    /// has to collapse to a single entry.
    pub fn hard_link(&self, from: &Path, to: impl Into<PathBuf>) {
        let mut state = self.state();
        if let Some(entry) = state.files.get(from).cloned() {
            state.files.insert(to.into(), entry);
        }
    }

    /// Moves a file, keeping its identity — which is what a real rename does.
    pub fn rename(&self, from: &Path, to: impl Into<PathBuf>) {
        let mut state = self.state();
        if let Some(entry) = state.files.remove(from) {
            state.files.insert(to.into(), entry);
        }
    }

    pub fn make_unreadable(&self, path: impl Into<PathBuf>) {
        self.state().unreadable = Some(path.into());
    }

    /// Parks the next `load` of `path` until the returned gate is released or
    /// dropped.
    ///
    /// The one piece of latency this fake models, and it earns its place: a
    /// `BTreeMap` resolves instantly, so without it no test can hold one note's
    /// load open while another note's completes — which is the only shape in
    /// which the editor and `NoteStore::current` can disagree.
    ///
    /// One gate parks one load. A second load of the same path runs unimpeded
    /// unless it is gated again.
    // No `#[must_use]`: `LoadGate` carries its own, and dropping the returned
    // gate on the spot would release the load immediately.
    pub fn block_load(&self, path: impl Into<PathBuf>) -> LoadGate {
        let (release, wait) = oneshot::channel();
        self.state().blocked.insert(path.into(), wait);
        LoadGate(release)
    }

    #[must_use]
    pub fn read_to_string(&self, path: &Path) -> Option<String> {
        self.state()
            .files
            .get(path)
            .map(|(text, _, _)| text.clone())
    }
}

/// Holds one `MemFs::load` parked. Releasing or dropping it lets that load
/// finish.
#[must_use]
pub struct LoadGate(oneshot::Sender<()>);

impl LoadGate {
    /// Lets the parked load finish. Dropping the gate does the same; use this
    /// name when the moment the load lands is the point of the test.
    pub fn release(self) {
        // Errs when the load was cancelled before it ever parked — which is
        // exactly what a superseded click is supposed to do to it.
        let _ = self.0.send(());
    }
}

#[async_trait]
impl Fs for MemFs {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let state = self.state();
        if state.unreadable.as_deref() == Some(path) {
            return Err(VaultError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            });
        }

        let mut entries = Vec::new();
        for (dir, (mtime, identity)) in &state.dirs {
            if dir.parent() == Some(path) {
                entries.push(DirEntry {
                    path: dir.clone(),
                    kind: Kind::Dir,
                    mtime: *mtime,
                    identity: *identity,
                });
            }
        }
        for (file, (_, mtime, identity)) in &state.files {
            if file.parent() == Some(path) {
                entries.push(DirEntry {
                    path: file.clone(),
                    kind: Kind::Note,
                    mtime: *mtime,
                    identity: *identity,
                });
            }
        }
        Ok(entries)
    }

    async fn load(&self, path: &Path) -> Result<Loaded> {
        // Taken out of the map, and the guard dropped, before the await: a
        // `MutexGuard` held across one would make this future non-`Send`, which
        // `async_trait` requires. A dropped sender resolves the receiver with
        // an error, which releases the load just as a send does.
        let blocked = self.state().blocked.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }

        let state = self.state();
        let Some((text, mtime, identity)) = state.files.get(path) else {
            return Err(VaultError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::NotFound),
            });
        };
        Ok(Loaded {
            text: crate::fs::normalize_line_endings(text),
            mtime: *mtime,
            identity: *identity,
        })
    }

    async fn save(&self, path: &Path, text: &str) -> Result<Mtime> {
        self.insert_file(path.to_path_buf(), text);
        Ok(Self::now())
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::MemFs;
    use crate::fs::{Fs, Kind, VaultError};
    use std::path::{Path, PathBuf};

    /// The scan walks with `read_dir` alone, so every directory on the way to
    /// a note has to exist without being inserted by hand.
    #[test]
    fn inserting_a_file_creates_its_parent_directories() {
        let fs = MemFs::new();
        fs.insert_file("/vault/sub/b.md", "b");

        let Ok(root) = block_on(fs.read_dir(Path::new("/vault"))) else {
            panic!("read_dir must succeed");
        };

        assert_eq!(root.len(), 1, "only the direct child is listed");
        assert_eq!(root[0].path, Path::new("/vault/sub"));
        assert_eq!(root[0].kind, Kind::Dir);
    }

    #[test]
    fn read_dir_lists_direct_children_and_separates_kinds() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_dir("/vault/sub");
        fs.insert_file("/vault/sub/b.md", "b");

        let Ok(entries) = block_on(fs.read_dir(Path::new("/vault"))) else {
            panic!("read_dir must succeed");
        };

        let of_kind = |wanted: Kind| -> Vec<PathBuf> {
            entries
                .iter()
                .filter(|entry| entry.kind == wanted)
                .map(|entry| entry.path.clone())
                .collect()
        };
        assert_eq!(of_kind(Kind::Dir), vec![PathBuf::from("/vault/sub")]);
        assert_eq!(
            of_kind(Kind::Note),
            vec![PathBuf::from("/vault/a.md")],
            "b.md is not a direct child of the root"
        );
    }

    #[test]
    fn an_unreadable_directory_reports_an_error_rather_than_an_empty_listing() {
        let fs = MemFs::new();
        fs.insert_dir("/vault/locked");
        fs.make_unreadable("/vault/locked");

        let result = block_on(fs.read_dir(Path::new("/vault/locked")));

        assert!(matches!(result, Err(VaultError::Io { .. })));
    }

    /// Identity tracks the file, not the name: a rename is the same file, a
    /// recreated file is a different one.
    #[test]
    fn a_rename_keeps_the_identity_and_a_recreated_file_gets_a_new_one() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        let Ok(before) = block_on(fs.load(Path::new("/vault/a.md"))) else {
            panic!("load must succeed");
        };

        fs.rename(Path::new("/vault/a.md"), "/vault/b.md");
        let Ok(renamed) = block_on(fs.load(Path::new("/vault/b.md"))) else {
            panic!("load must succeed after the rename");
        };
        assert_eq!(renamed.identity, before.identity);

        fs.remove(Path::new("/vault/b.md"));
        fs.insert_file("/vault/b.md", "second");
        let Ok(recreated) = block_on(fs.load(Path::new("/vault/b.md"))) else {
            panic!("load must succeed after the recreation");
        };
        assert_ne!(recreated.identity, before.identity);
    }

    #[test]
    fn save_round_trips_and_load_normalizes_line_endings() {
        let fs = MemFs::new();

        let Ok(_) = block_on(fs.save(Path::new("/vault/a.md"), "one\r\ntwo")) else {
            panic!("save must succeed");
        };
        let Ok(loaded) = block_on(fs.load(Path::new("/vault/a.md"))) else {
            panic!("load must succeed");
        };

        assert_eq!(loaded.text, "one\ntwo");
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("one\r\ntwo"),
            "the stored bytes are untouched; normalization happens on load"
        );
    }
}
