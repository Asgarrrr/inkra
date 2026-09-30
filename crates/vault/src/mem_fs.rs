//! An in-memory `Fs` for tests: a `BTreeMap`, injectable errors and gates on
//! both I/O paths, not a simulated kernel.

mod errors;
mod faults;
mod fixture;
mod watch;

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use futures::channel::oneshot;

use crate::fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp};
use crate::temp::temp_path;
use crate::watch::Watch;

use errors::{io, stamp_of};
pub use faults::Gate;

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
    dirs: BTreeMap<PathBuf, (Mtime, FileIdentity)>,
    next_inode: u64,
    unreadable: Option<PathBuf>,
    unwritable: BTreeSet<PathBuf>,
    clock: u64,
    /// Separate from `unwritable`: that fails a write to a file that already
    /// exists, but a create/rename has no file at the destination yet.
    uncreatable: BTreeSet<PathBuf>,
    blocked_loads: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// Separate from `unwritable`: a note can be writable and still not
    /// removable (no trash on the volume, say).
    untrashable: BTreeSet<PathBuf>,
    /// A move, not a drop, so a trashed note stays recoverable in tests.
    trashed: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
    /// Loads started per path, found or not.
    loads: BTreeMap<PathBuf, usize>,
    blocked_read_dirs: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    blocked_after_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// Listings started per path, found or not.
    read_dirs: BTreeMap<PathBuf, usize>,
    watches: Vec<watch::Watching>,
}

impl State {
    /// As `RealFs`: a folder renamed or removed under a walk fails, it does not read as empty.
    fn readable_dir(&self, path: &Path) -> Result<()> {
        if self.unreadable.as_deref() == Some(path) {
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
    fn put_file(state: &mut State, path: PathBuf, text: String, identity: Option<FileIdentity>) {
        Self::insert_parents(state, &path);
        let identity = identity
            .or_else(|| state.files.get(&path).map(|(_, _, id)| *id))
            .unwrap_or_else(|| Self::fresh_identity(state));
        let mtime = Self::tick(state);
        state.files.insert(path, (text, mtime, identity));
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
            // As `RealFs` does: only notes are listed.
            let note = file
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            if note && file.parent() == Some(path) {
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
        let Some((text, mtime, identity)) = state.files.get(path) else {
            return Err(io(path, ErrorKind::NotFound));
        };
        Ok(Loaded {
            text: crate::fs::normalize_line_endings(text),
            stamp: stamp_of(text, *mtime),
            identity: *identity,
        })
    }

    /// Mints a fresh identity, unlike `insert_file`: `RealFs::save` renames a
    /// temp file over the destination, so the inode changes too.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        let blocked = self.state().blocked_saves.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        {
            let state = self.state();
            if state.unwritable.contains(path) {
                return Err(io(path, ErrorKind::PermissionDenied));
            }
            // As `RealFs`: the temporary file cannot be created in a missing folder.
            if path
                .parent()
                .is_some_and(|parent| !state.dirs.contains_key(parent))
            {
                return Err(io(path, ErrorKind::NotFound));
            }
        }

        // Guard scoped to this block: `state()`'s mutex is not reentrant.
        let preserved = {
            let state = self.state();
            match state.files.get(path) {
                Some((existing, mtime, _)) if stamp_of(existing, *mtime) != expected => {
                    let destination = crate::conflict::conflict_path(path, *mtime, |candidate| {
                        state.files.contains_key(candidate)
                    });
                    Some((destination, existing.clone()))
                }
                _ => None,
            }
        };
        if let Some((destination, bytes)) = preserved.clone() {
            self.insert_file(destination, bytes);
        }

        Self::put_file(&mut self.state(), path.to_path_buf(), text.to_owned(), None);

        let (stamp, identity, blocked) = {
            let mut state = self.state();
            let identity = Self::fresh_identity(&mut state);
            let mtime = Self::tick(&mut state);
            let Some((stored_text, stored_mtime, stored_identity)) = state.files.get_mut(path)
            else {
                return Err(io(path, ErrorKind::NotFound));
            };
            *stored_mtime = mtime;
            *stored_identity = identity;
            let stamp = stamp_of(stored_text, mtime);
            // As `RealFs::save`: a temporary file, renamed onto the note.
            state.changed(&temp_path(path));
            state.changed(path);
            (stamp, identity, state.blocked_after_saves.remove(path))
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        Ok(Saved {
            stamp,
            identity,
            preserved: preserved.map(|(destination, _)| destination),
        })
    }

    async fn create_parents(&self, path: &Path) -> Result<()> {
        Self::insert_parents(&mut self.state(), path);
        Ok(())
    }

    async fn replace(&self, path: &Path, text: &str) -> Result<()> {
        self.insert_file(path, text);
        Ok(())
    }

    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity> {
        let mut state = self.state();
        if state.uncreatable.contains(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        if state.files.contains_key(path) || state.dirs.contains_key(path) {
            return Err(io(path, ErrorKind::AlreadyExists));
        }

        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        match kind {
            Kind::Note => {
                state
                    .files
                    .insert(path.to_path_buf(), (String::new(), mtime, identity));
            }
            Kind::Dir => {
                state.dirs.insert(path.to_path_buf(), (mtime, identity));
            }
        }
        state.changed(path);
        Ok(identity)
    }

    /// Moves the stored entry rather than rebuilding it, so the identity
    /// travels with the file as an inode does under `rename(2)`. The subtree
    /// moves with a directory for the same reason.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let mut state = self.state();
        if state.uncreatable.contains(to) {
            return Err(io(to, ErrorKind::PermissionDenied));
        }
        // Byte equality only: unlike a real volume, this fake does not fold
        // case or Unicode normalization.
        if from == to {
            return Ok(());
        }
        if state.files.contains_key(to) || state.dirs.contains_key(to) {
            return Err(io(to, ErrorKind::AlreadyExists));
        }

        if let Some(entry) = state.files.remove(from) {
            state.files.insert(to.to_path_buf(), entry);
            state.changed(from);
            state.changed(to);
            return Ok(());
        }
        let Some(entry) = state.dirs.remove(from) else {
            return Err(io(from, ErrorKind::NotFound));
        };
        state.dirs.insert(to.to_path_buf(), entry);

        let moved: Vec<_> = state
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
            if let Some(entry) = state.files.remove(&path) {
                state.files.insert(destination, entry);
            }
        }
        let moved: Vec<_> = state
            .dirs
            .keys()
            .filter(|path| path.as_path() != to && path.starts_with(from))
            .cloned()
            .collect();
        for path in moved {
            let Ok(relative) = path.strip_prefix(from) else {
                continue;
            };
            let destination = to.join(relative);
            if let Some(entry) = state.dirs.remove(&path) {
                state.dirs.insert(destination, entry);
            }
        }
        // As FSEvents: the two names, not the subtree that moved with them.
        state.changed(from);
        state.changed(to);
        Ok(())
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
        Ok(self.start_watch(root))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod watch_tests;
