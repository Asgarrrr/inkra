//! An in-memory `Fs` for tests: a `BTreeMap`, injectable errors and gates on
//! both I/O paths, not a simulated kernel. Zed's `FakeFs` is 2207 lines
//! (`vendor/zed-reference/crates/fs/src/fs.rs:1440-3646`) because it amortizes
//! over hundreds of scanner tests; ours does not.
//!
//! Compiled under `--all-features`, so `unwrap_used = "deny"` applies here too.

mod faults;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use futures::channel::oneshot;

use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError,
};

pub use faults::Gate;

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
    dirs: BTreeMap<PathBuf, (Mtime, FileIdentity)>,
    next_inode: u64,
    /// `read_dir` on exactly this path fails. Models an unreadable directory;
    /// its children are unreachable anyway, since the walk never descends past
    /// a directory it could not read.
    unreadable: Option<PathBuf>,
    /// Every `save` of these paths fails. Models a read-only file, a full disk,
    /// or a directory the process cannot write into. See `make_unwritable`.
    unwritable: BTreeSet<PathBuf>,
    /// Seconds past the base, advanced by every mutation. A constant here made
    /// every write share one mtime, which made a stale stamp unreachable and
    /// conflict detection untestable.
    clock: u64,
    /// Every `create` or `rename` **into** one of these paths fails. Models a
    /// directory the process cannot write into, which `unwritable` cannot:
    /// that one fails a write to a file that already exists, and neither a
    /// create nor a rename has a file at the destination yet.
    uncreatable: BTreeSet<PathBuf>,
    /// The next `load` of each of these paths parks until its `Gate` is
    /// released or dropped. See `MemFs::block_load`.
    blocked_loads: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// The same, for the next `save` of each path. See `MemFs::block_save`.
    blocked_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>,
    /// Every `trash` of these paths fails. Models a Trash the platform refuses
    /// — no trash on the volume, or a permission the process does not have.
    /// Separate from `unwritable`: a note can be perfectly writable and still
    /// not removable.
    untrashable: BTreeSet<PathBuf>,
    /// What `trash` took, by the path it was taken from. A move, never a drop:
    /// the whole promise of a trash is that the bytes survive, and a fake that
    /// forgot them would let a test assert a recovery that cannot happen.
    trashed: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
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

    /// Recovers the guard from a poisoned lock rather than panicking: the lock
    /// is only poisoned when another test thread already panicked, and its
    /// failure is the one worth reading.
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

    /// Advances one second and returns the new time. Takes `&mut State` rather
    /// than nothing, because a clock that does not move is not a clock — and a
    /// fixed one made every write of a path compare equal to every other,
    /// which put conflict detection out of reach of every test.
    fn tick(state: &mut State) -> Mtime {
        state.clock += 1;
        Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + state.clock))
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
            let mtime = Self::tick(&mut state);
            state.dirs.insert(dir.to_path_buf(), (mtime, identity));
            parent = dir.parent();
        }
        let existing = state.files.get(&path).map(|(_, _, id)| *id);
        let identity = existing.unwrap_or_else(|| Self::fresh_identity(&mut state));
        let mtime = Self::tick(&mut state);
        state.files.insert(path, (text.into(), mtime, identity));
    }

    pub fn insert_dir(&self, path: impl Into<PathBuf>) {
        let path = path.into();
        let mut state = self.state();
        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        state.dirs.insert(path, (mtime, identity));
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

    /// What `trash` took from `path`, if anything. The only way a test can
    /// assert a trashed note is recoverable rather than destroyed.
    #[must_use]
    pub fn trashed(&self, path: &Path) -> Option<String> {
        self.state()
            .trashed
            .get(path)
            .map(|(text, _, _)| text.clone())
    }

    #[must_use]
    pub fn read_to_string(&self, path: &Path) -> Option<String> {
        self.state()
            .files
            .get(path)
            .map(|(text, _, _)| text.clone())
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
        let blocked = self.state().blocked_loads.remove(path);
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
            stamp: stamp_of(text, *mtime),
            identity: *identity,
        })
    }

    /// Mints a **fresh** identity, unlike `insert_file`, which keeps the path's
    /// existing one. That asymmetry is the point: `insert_file` models an
    /// external writer rewriting a file in place, while `RealFs::save` renames a
    /// temp file over the destination and so changes the inode. A fake that kept
    /// the identity here would hide every re-keying bug from every `app` test,
    /// since they all run on `MemFs`.
    ///
    /// No link count is modelled, so this always takes the replacing path —
    /// `RealFs::save` falls back to writing in place when `nlink > 1`.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        // Parked before the insert, and the guard dropped before the await for
        // the same `Send` reason as `load`. A gated write must not have landed
        // while it is still held: a test that releases the gate is asserting
        // that *this* write is the one reaching the file.
        let blocked = self.state().blocked_saves.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        // Checked after the gate, so a blocked write still fails where a real
        // one would — at the end, having consumed the time.
        if self.state().unwritable.contains(path) {
            return Err(VaultError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            });
        }

        // Preserved before the replace, in the order `RealFs` uses and for the
        // same reason. A fake that reversed it would stay green while the real
        // one lost data.
        //
        // The guard is scoped to this block and dropped before `insert_file`,
        // which takes the same lock: `state()` is a `std::sync::Mutex` and is
        // not reentrant.
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

        self.insert_file(path.to_path_buf(), text);

        let mut state = self.state();
        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        let Some((stored_text, stored_mtime, stored_identity)) = state.files.get_mut(path) else {
            return Err(VaultError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::NotFound),
            });
        };
        *stored_mtime = mtime;
        *stored_identity = identity;
        let stamp = stamp_of(stored_text, mtime);
        Ok(Saved {
            stamp,
            identity,
            preserved: preserved.map(|(destination, _)| destination),
        })
    }

    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity> {
        let mut state = self.state();
        if state.uncreatable.contains(path) {
            return Err(permission_denied(path));
        }
        if state.files.contains_key(path) || state.dirs.contains_key(path) {
            return Err(already_exists(path));
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
        Ok(identity)
    }

    /// Moves the stored entry rather than rebuilding it, so the identity travels
    /// with the file exactly as an inode does under `rename(2)`. A fake that
    /// minted a fresh one here would hide every `EntryId`-survives-a-rename bug
    /// from every `app` test, since they all run on this.
    ///
    /// The subtree moves with a directory, for the same reason: `rename(2)` is
    /// one call on the directory entry and never touches the children's inodes.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let mut state = self.state();
        if state.uncreatable.contains(to) {
            return Err(permission_denied(to));
        }
        // Byte equality, which on this fake is the *only* way two paths name one
        // file: the maps are keyed by raw `PathBuf`. A real volume also folds
        // case and Unicode normalization, and `RealFs::rename` answers that by
        // inode. Recorded rather than modelled — see the design's note on the
        // deferred case-insensitive `MemFs`.
        if from == to {
            return Ok(());
        }
        if state.files.contains_key(to) || state.dirs.contains_key(to) {
            return Err(already_exists(to));
        }

        if let Some(entry) = state.files.remove(from) {
            state.files.insert(to.to_path_buf(), entry);
            return Ok(());
        }
        let Some(entry) = state.dirs.remove(from) else {
            return Err(not_found(from));
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
        Ok(())
    }

    async fn trash(&self, path: &Path) -> Result<()> {
        let mut state = self.state();
        if state.untrashable.contains(path) {
            return Err(permission_denied(path));
        }
        let Some(entry) = state.files.remove(path) else {
            return Err(not_found(path));
        };
        state.trashed.insert(path.to_path_buf(), entry);
        Ok(())
    }
}

fn not_found(path: &Path) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source: io::Error::from(io::ErrorKind::NotFound),
    }
}

fn already_exists(path: &Path) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source: io::Error::from(io::ErrorKind::AlreadyExists),
    }
}

fn permission_denied(path: &Path) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source: io::Error::from(io::ErrorKind::PermissionDenied),
    }
}

/// The stamp a stored file would report. `size` is the byte length of the text
/// as stored, which is what `RealFs` reports for the same content.
fn stamp_of(text: &str, mtime: Mtime) -> Stamp {
    Stamp {
        mtime,
        size: text.len() as u64,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use futures::FutureExt;
    use futures::executor::block_on;

    use super::MemFs;
    use crate::fs::{Fs, Kind, Mtime, Stamp, VaultError};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    /// `expect` is denied in this file, so the load goes through the `let Ok`
    /// shape the rest of the module uses.
    fn stamp_of(fs: &MemFs, path: &str) -> Stamp {
        let Ok(loaded) = block_on(fs.load(Path::new(path))) else {
            panic!("load must succeed");
        };
        loaded.stamp
    }

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

        block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
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

    /// The injector the `SaveFailed` path needs: without it every write
    /// succeeds and the error arm is unreachable from any test.
    #[test]
    fn an_unwritable_file_reports_an_error_and_keeps_its_old_text() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        fs.make_unwritable("/vault/a.md");

        let stamp = stamp_of(&fs, "/vault/a.md");
        let result = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp));

        assert!(matches!(result, Err(VaultError::Io { .. })));
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("before"),
            "a failed write leaves the file as it was"
        );
    }

    /// The gate has to hold the *write*, not just the future: a test that
    /// releases it is asserting the write lands at that moment and not before.
    #[test]
    fn a_blocked_save_does_not_reach_the_file_until_the_gate_releases() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");

        let gate = fs.block_save("/vault/a.md");
        let stamp = stamp_of(&fs, "/vault/a.md");
        let mut write = Box::pin(fs.save(Path::new("/vault/a.md"), "after", stamp));
        // Polled once so the future reaches the gate and parks there.
        assert!(write.as_mut().now_or_never().is_none());
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("before"),
            "the write is in flight and must not have landed"
        );

        gate.release();
        assert!(block_on(write).is_ok());
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("after")
        );
    }

    /// `now` used to be a constant, which made every stamp of a path equal to
    /// every other and every conflict undetectable. This is what pins the
    /// clock: delete `tick` and this is the test that says so.
    #[test]
    fn two_writes_of_one_path_get_different_stamps() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "one");
        let first = stamp_of(&fs, "/vault/a.md");
        fs.insert_file("/vault/a.md", "two");
        let second = stamp_of(&fs, "/vault/a.md");

        assert_ne!(first, second);
    }

    /// The fake must preserve in the same order the real one does. Reversed, it
    /// would stay green while `RealFs` lost data.
    #[test]
    fn a_stale_stamp_preserves_the_external_text() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let stamp = stamp_of(&fs, "/vault/a.md");

        // `insert_file` keeps the path's identity, exactly as a second process
        // rewriting a file in place would.
        fs.insert_file("/vault/a.md", "theirs");

        let Ok(saved) = block_on(fs.save(Path::new("/vault/a.md"), "ours", stamp)) else {
            panic!("save must succeed");
        };

        let Some(preserved) = saved.preserved else {
            panic!("a stale stamp must preserve");
        };
        assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("ours")
        );
    }

    /// The comparison site, not the derived `PartialEq`. Every other conflict
    /// test moves the file's mtime *forward*, so all of them stay green if
    /// `!=` is relaxed to `>`. This one has the file older than the
    /// expectation — the shape a sync client produces when it preserves an
    /// original modified date — and is the only test that fails under that
    /// relaxation.
    #[test]
    fn a_file_older_than_the_expectation_still_conflicts() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "theirs");
        let actual = stamp_of(&fs, "/vault/a.md");

        let from_the_future = Stamp {
            mtime: Mtime(actual.mtime.0 + Duration::from_mins(1)),
            size: actual.size,
        };
        let Ok(saved) = block_on(fs.save(Path::new("/vault/a.md"), "ours", from_the_future)) else {
            panic!("save must succeed");
        };

        let Some(preserved) = saved.preserved else {
            panic!("a file older than the expectation is still not the file we expected");
        };
        assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
    }

    #[test]
    fn the_stamp_a_save_returns_is_accepted_by_the_next_save() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let stamp = stamp_of(&fs, "/vault/a.md");

        let Ok(first) = block_on(fs.save(Path::new("/vault/a.md"), "one", stamp)) else {
            panic!("the first save must succeed");
        };
        let Ok(second) = block_on(fs.save(Path::new("/vault/a.md"), "two", first.stamp)) else {
            panic!("the second save must succeed");
        };

        assert!(second.preserved.is_none());
    }

    #[test]
    fn create_refuses_a_name_that_is_taken() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "alpha");

        assert!(block_on(fs.create(Path::new("/vault/a.md"), Kind::Note)).is_err());
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("alpha")
        );
    }

    #[test]
    fn create_mints_an_identity_the_scan_reports() {
        let fs = MemFs::new();
        fs.insert_dir("/vault");

        let identity = block_on(fs.create(Path::new("/vault/new.md"), Kind::Note)).expect("create");

        let entries = block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].identity, identity);
    }

    /// Byte-for-byte, so `Vault::apply` keeps the `EntryId` across the move.
    #[test]
    fn rename_carries_the_identity_to_the_new_path() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "alpha");
        let before = block_on(fs.load(Path::new("/vault/a.md")))
            .expect("load")
            .identity;

        block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");

        let after = block_on(fs.load(Path::new("/vault/b.md"))).expect("load");
        assert_eq!(after.identity, before);
        assert_eq!(after.text, "alpha");
    }

    #[test]
    fn rename_refuses_an_occupied_destination_and_keeps_both_files() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "mine");
        fs.insert_file("/vault/b.md", "theirs");

        assert!(block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).is_err());

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("mine")
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/b.md")).as_deref(),
            Some("theirs")
        );
    }

    /// `rename(2)` is one call on the directory entry and never touches the
    /// children's inodes, so a fake that dropped the subtree would make every
    /// folder rename look like a mass delete.
    #[test]
    fn renaming_a_folder_moves_its_subtree() {
        let fs = MemFs::new();
        fs.insert_file("/vault/projects/plan.md", "p");
        fs.insert_file("/vault/projects/deep/x.md", "x");

        block_on(fs.rename(Path::new("/vault/projects"), Path::new("/vault/archive")))
            .expect("rename");

        assert_eq!(
            fs.read_to_string(Path::new("/vault/archive/plan.md"))
                .as_deref(),
            Some("p")
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/archive/deep/x.md"))
                .as_deref(),
            Some("x")
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/projects/plan.md")),
            None
        );
    }

    /// A trash the user can recover from. A fake that dropped the bytes would
    /// let a test claim a recovery the real platform makes and this one does not.
    #[test]
    fn trash_takes_the_note_out_of_the_vault_and_keeps_its_bytes() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "alpha");

        block_on(fs.trash(Path::new("/vault/a.md"))).expect("trash");

        assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
        assert!(
            block_on(fs.read_dir(Path::new("/vault")))
                .expect("read_dir")
                .is_empty()
        );
        assert_eq!(
            fs.trashed(Path::new("/vault/a.md")).as_deref(),
            Some("alpha")
        );
    }

    #[test]
    fn trash_of_a_missing_note_reports_an_error() {
        let fs = MemFs::new();
        fs.insert_dir("/vault");

        assert!(block_on(fs.trash(Path::new("/vault/gone.md"))).is_err());
    }

    #[test]
    fn save_round_trips_and_load_normalizes_line_endings() {
        let fs = MemFs::new();

        // The path does not exist yet, so the expectation is never compared:
        // `save` stats nothing when there is no file to stat.
        let absent = Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH),
            size: 0,
        };
        let Ok(_) = block_on(fs.save(Path::new("/vault/a.md"), "one\r\ntwo", absent)) else {
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
