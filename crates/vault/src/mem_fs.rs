//! An in-memory `Fs` for tests: a `BTreeMap`, injectable errors and gates on
//! both I/O paths, not a simulated kernel.

mod errors;
mod faults;
mod fixture;
mod rename;
mod save;
mod watch;

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use futures::channel::oneshot;

use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, MAX_TEXT_BYTES, Mtime, New, Result, Saved, Stamp,
    VaultError, listable,
};
use crate::watch::Watch;

use errors::{io, stamp_of};
pub use faults::Gate;

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, (Vec<u8>, Mtime, FileIdentity)>,
    dirs: BTreeMap<PathBuf, (Mtime, FileIdentity)>,
    next_inode: u64,
    unreadable: BTreeSet<PathBuf>,
    unwritable: BTreeSet<PathBuf>,
    clock: u64,
    /// Separate from `unwritable`: that fails a write to a file that already
    /// exists, but a create/rename has no file at the destination yet.
    uncreatable: BTreeSet<PathBuf>,
    blocked_loads: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_read_bytes: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_replaces: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_creates: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// Separate from `unwritable`: a note can be writable and still not
    /// removable (no trash on the volume, say).
    untrashable: BTreeSet<PathBuf>,
    /// A move, not a drop, so a trashed note stays recoverable in tests.
    trashed: BTreeMap<PathBuf, (Vec<u8>, Mtime, FileIdentity)>,
    /// Loads started per path, found or not.
    loads: BTreeMap<PathBuf, usize>,
    /// Byte reads started per path, found or not.
    reads: BTreeMap<PathBuf, usize>,
    blocked_read_dirs: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_after_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_after_renames: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// Listings started per path, found or not.
    read_dirs: BTreeMap<PathBuf, usize>,
    watches: Vec<watch::Watching>,
    unwatchable: BTreeSet<PathBuf>,
    failing_saves: BTreeMap<PathBuf, (u32, ErrorKind)>,
    failing_after_preserve: BTreeMap<PathBuf, (u32, ErrorKind)>,
    failing_replaces: BTreeMap<PathBuf, (u32, ErrorKind)>,
    failing_removes: BTreeMap<PathBuf, (u32, ErrorKind)>,
}

impl State {
    /// As `RealFs`: a folder renamed or removed under a walk fails, it does not read as empty.
    fn readable_dir(&self, path: &Path) -> Result<()> {
        if self.unreadable.contains(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        if self.files.contains_key(path) {
            return Err(io(path, ErrorKind::NotADirectory));
        }
        if !self.dirs.contains_key(path) {
            return Err(io(path, ErrorKind::NotFound));
        }
        Ok(())
    }

    /// A file or a folder: what `RealFs` asks `symlink_metadata`.
    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.dirs.contains_key(path)
    }

    /// As `RealFs`: a create, a rename or a save's temp file cannot land in a missing folder.
    fn missing_parent(&self, path: &Path) -> bool {
        path.parent()
            .is_some_and(|parent| !self.dirs.contains_key(parent))
    }

    /// As `RealFs`: an unreadable, missing or over-`max_len` file is refused before a byte is handed out.
    fn readable_file(&self, path: &Path, max_len: u64) -> Result<&(Vec<u8>, Mtime, FileIdentity)> {
        self.refused_load(path)?;
        let Some(file) = self.files.get(path) else {
            return Err(io(path, ErrorKind::NotFound));
        };
        if file.0.len() as u64 > max_len {
            return Err(io(path, ErrorKind::FileTooLarge));
        }
        Ok(file)
    }
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

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn fresh_identity(state: &mut State) -> FileIdentity {
        let inode = state.next_inode;
        state.next_inode += 1;
        FileIdentity { device: 1, inode }
    }

    /// Advances one second. A fixed clock would make every write of a path
    /// compare equal, putting conflict detection out of reach of any test.
    fn tick(state: &mut State) -> Mtime {
        state.clock += 1;
        Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + state.clock))
    }

    /// `None` keeps the path's identity, or mints one for a new path.
    fn put_file(
        state: &mut State,
        path: PathBuf,
        bytes: Vec<u8>,
        identity: Option<FileIdentity>,
    ) -> Mtime {
        Self::insert_parents(state, &path);
        let identity = identity
            .or_else(|| state.files.get(&path).map(|(_, _, id)| *id))
            .unwrap_or_else(|| Self::fresh_identity(state));
        let mtime = Self::tick(state);
        state.files.insert(path, (bytes, mtime, identity));
        mtime
    }

    fn insert_parents(state: &mut State, path: &Path) {
        let mut parent = path.parent();
        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() || state.dirs.contains_key(dir) {
                break;
            }
            let identity = Self::fresh_identity(state);
            let mtime = Self::tick(state);
            state.dirs.insert(dir.to_path_buf(), (mtime, identity));
            state.changed(dir);
            parent = dir.parent();
        }
    }
}

#[async_trait]
impl Fs for MemFs {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let blocked = {
            let mut state = self.state();
            *state.read_dirs.entry(path.to_path_buf()).or_default() += 1;
            state.blocked_read_dirs.remove(path)
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }

        let state = self.state();
        state.readable_dir(path)?;

        let child =
            |entry: &Path| entry.parent() == Some(path) && entry.file_name().is_some_and(listable);
        let mut entries = Vec::new();
        for (dir, (mtime, identity)) in &state.dirs {
            if child(dir) {
                entries.push(DirEntry {
                    path: dir.clone(),
                    kind: Kind::Dir,
                    mtime: *mtime,
                    size: 0,
                    identity: *identity,
                });
            }
        }
        for (file, (bytes, mtime, identity)) in &state.files {
            if child(file) {
                entries.push(DirEntry {
                    path: file.clone(),
                    kind: Kind::of_file(file),
                    mtime: *mtime,
                    size: bytes.len() as u64,
                    identity: *identity,
                });
            }
        }
        Ok(entries)
    }

    /// Counts no listing: `read_dirs` counts the vault's walk.
    async fn list_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let state = self.state();
        state.readable_dir(dir)?;
        Ok(state
            .files
            .keys()
            .filter(|file| file.parent() == Some(dir) && file.file_name().is_some_and(listable))
            .cloned()
            .collect())
    }

    async fn check_dir(&self, path: &Path) -> Result<()> {
        self.state().readable_dir(path)
    }

    async fn load(&self, path: &Path) -> Result<Loaded> {
        let blocked = {
            let mut state = self.state();
            *state.loads.entry(path.to_path_buf()).or_default() += 1;
            state.blocked_loads.remove(path)
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }

        let state = self.state();
        let (bytes, mtime, identity) = state.readable_file(path, MAX_TEXT_BYTES)?;
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Err(VaultError::NotUtf8 {
                path: path.to_path_buf(),
            });
        };
        Ok(Loaded {
            text: crate::fs::normalize_line_endings(text),
            stamp: stamp_of(bytes, *mtime),
            identity: *identity,
        })
    }

    /// Counts in `reads`, never in `loads`: reading a file's bytes is no note's load.
    async fn read_bytes(&self, path: &Path, max_len: u64) -> Result<Vec<u8>> {
        let blocked = {
            let mut state = self.state();
            *state.reads.entry(path.to_path_buf()).or_default() += 1;
            state.blocked_read_bytes.remove(path)
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        let state = self.state();
        let (bytes, _, _) = state.readable_file(path, max_len)?;
        Ok(bytes.clone())
    }

    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        self.save_text(path, text, expected).await
    }

    async fn create_parents(&self, path: &Path) -> Result<()> {
        Self::insert_parents(&mut self.state(), path);
        Ok(())
    }

    async fn replace_bytes(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let blocked = self.state().blocked_replaces.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        {
            let mut state = self.state();
            state.injected_replace_failure(path)?;
            if state.unwritable.contains(path) {
                return Err(io(path, ErrorKind::PermissionDenied));
            }
        }
        self.insert_bytes(path, bytes);
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        let mut state = self.state();
        state.injected_remove_failure(path)?;
        if state.dirs.contains_key(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        if state.files.remove(path).is_some() {
            state.changed(path);
        }
        Ok(())
    }

    async fn create(&self, path: &Path, new: New) -> Result<FileIdentity> {
        let blocked = self.state().blocked_creates.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        let mut state = self.state();
        if state.uncreatable.contains(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        if state.exists(path) {
            return Err(io(path, ErrorKind::AlreadyExists));
        }
        if state.missing_parent(path) {
            return Err(io(path, ErrorKind::NotFound));
        }

        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        match new {
            New::Note => {
                state
                    .files
                    .insert(path.to_path_buf(), (Vec::new(), mtime, identity));
            }
            New::Dir => {
                state.dirs.insert(path.to_path_buf(), (mtime, identity));
            }
        }
        state.changed(path);
        Ok(identity)
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let result = self.state().rename(from, to);
        let blocked = result
            .is_ok()
            .then(|| self.state().blocked_after_renames.remove(from))
            .flatten();
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        result
    }

    async fn trash(&self, path: &Path) -> Result<()> {
        let mut state = self.state();
        if state.untrashable.contains(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        let Some(entry) = state.files.remove(path) else {
            return Err(io(path, ErrorKind::NotFound));
        };
        state.trashed.insert(path.to_path_buf(), entry);
        state.changed(path);
        Ok(())
    }

    fn watch(&self, root: &Path) -> Result<Watch> {
        self.state().refused_watch(root)?;
        Ok(self.start_watch(root))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod watch_tests;

#[cfg(test)]
mod fault_tests;
