# inkra-vault Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a folder of markdown notes, show them as a tree, click one to load it into the editor, and `⌘S` to write it back.

**Architecture:** A new gpui-free crate `inkra-vault` owns an async `Fs` trait and the vault model, keyed by `(device, inode)` rather than by path. `inkra-app` owns the only implementations that touch the platform, a `NoteStore` holding strong buffer handles, and the sidebar. `inkra-text` gains `Buffer::version()`, with the id on the `Edit` rather than the `Transaction`.

**Tech Stack:** Rust 2024, `async-trait` (one new crate; `proc-macro2`/`quote`/`syn` are already present), `thiserror`, `futures::executor::block_on` for vault tests, gpui's `cx.background_spawn` for orchestration.

**Spec:** `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`. Read its "What the review changed" section before starting — three critical defects are designed against, and Tasks 6, 11 and 12 each carry the regression test that proves one of them stays fixed.

**Verification:** `./scripts/check.sh` must be green at every commit. It runs fmt, clippy `-D warnings` with `--all-features`, the gpui-isolation loop, tests, and a release build. `unwrap_used` and `expect_used` are `deny` at the workspace level, including in test-support code, which clippy compiles because of `--all-features`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/vault/Cargo.toml` | New crate manifest, `test-support` feature, self dev-dependency |
| `crates/vault/src/lib.rs` | Module wiring and the public surface |
| `crates/vault/src/path.rs` | `RelPath` and its validation |
| `crates/vault/src/fs.rs` | `Fs` trait, `DirEntry`, `Loaded`, `Mtime`, `FileIdentity`, `VaultError` |
| `crates/vault/src/real_fs.rs` | `RealFs`, blocking `std::fs` inside the future |
| `crates/vault/src/mem_fs.rs` | `MemFs`, behind `test-support` |
| `crates/vault/src/vault.rs` | `Entry`, `EntryId`, `Vault`, the scan, the comparator |
| `crates/text/src/buffer.rs` | Gains `next_edit_id`, `Edit::id`, `Buffer::version()` |
| `crates/app/src/vault.rs` | `NoteStore`, `actions!(vault, [Save])` |
| `crates/app/src/sidebar.rs` | The tree: visible rows, expansion, `uniform_list` |
| `crates/app/src/editor/mod.rs` | Gains `open_buffer`, the third gate |
| `crates/app/src/shell.rs` | Owns `NoteStore`, hosts the sidebar, handles `Save` |
| `crates/app/src/keymap.rs` | Binds `cmd-s` at the `AppShell` context |
| `crates/app/src/main.rs` | Takes a path argument, builds the vault |

---

# Phase 1 — Walking skeleton: a file reaches the editor

At the end of this phase, `cargo run -p inkra-app -- some-note.md` shows that file's text. No vault model yet, no save.

### Task 1: Scaffold `inkra-vault` and wire the workspace

The spec calls this "four edits, and the loop addition fails outright without the first". Do all four.

**Files:**
- Create: `crates/vault/Cargo.toml`, `crates/vault/src/lib.rs`
- Modify: `Cargo.toml:3`, `Cargo.toml:8`, `scripts/check.sh:9`, `scripts/check.sh:29`

- [ ] **Step 1: Create the manifest**

`crates/vault/Cargo.toml`:

```toml
[package]
name = "inkra-vault"
version = "0.1.0"
edition = "2024"

[features]
# `MemFs` is gated so it never reaches a release binary. It must still compile
# under `cargo test --workspace`, which check.sh runs WITHOUT --all-features —
# hence the self dev-dependency below. Same pattern app uses for gpui.
test-support = []

[dependencies]
async-trait = "0.1"
thiserror = "2"

[dev-dependencies]
inkra-vault = { path = ".", features = ["test-support"] }
futures = "0.3"
proptest = "1"

# Workspace lints are opt-in per crate. Without this, unwrap_used and
# expect_used are not denied here.
[lints]
workspace = true
```

- [ ] **Step 2: Create the crate root**

`crates/vault/src/lib.rs`:

```rust
//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.
//!
//! No gpui, no workspace dependency — `Fs::load` returns a `String` and `app`
//! builds the `Buffer`. See
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.

mod path;

pub use path::RelPath;
```

- [ ] **Step 3: Wire the workspace**

`Cargo.toml:3` becomes:

```toml
members = ["crates/text", "crates/editor", "crates/app", "crates/vault"]
```

`Cargo.toml:8`, inside `[workspace.dependencies]`, add:

```toml
inkra-vault = { path = "crates/vault" }
```

- [ ] **Step 4: Extend check.sh**

`scripts/check.sh:9` becomes:

```bash
cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -- --check
```

`scripts/check.sh:29` becomes:

```bash
for crate in inkra-text inkra-editor inkra-vault; do
```

- [ ] **Step 5: Verify the whole gate still passes**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`. The gpui-isolation loop now names three crates and `inkra-vault` has no gpui edge, so it passes trivially — that is the point, it is guarded from the first commit rather than retrofitted.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock scripts/check.sh crates/vault
git commit -m "Add the inkra-vault crate, guarded from the first commit"
```

---

### Task 2: `RelPath`, a path that cannot escape the vault

**Files:**
- Create: `crates/vault/src/path.rs`

- [ ] **Step 1: Write the failing tests**

`crates/vault/src/path.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_relative_path_is_accepted() {
        let path = RelPath::new(Path::new("notes/today.md")).unwrap();
        assert_eq!(path.as_path(), Path::new("notes/today.md"));
    }

    #[test]
    fn an_absolute_path_is_rejected() {
        assert!(RelPath::new(Path::new("/etc/passwd")).is_none());
    }

    #[test]
    fn a_parent_component_is_rejected_anywhere_in_the_path() {
        assert!(RelPath::new(Path::new("../secrets.md")).is_none());
        assert!(RelPath::new(Path::new("notes/../../secrets.md")).is_none());
    }

    #[test]
    fn the_depth_is_the_component_count() {
        let path = RelPath::new(Path::new("a/b/c.md")).unwrap();
        assert_eq!(path.depth(), 2);
    }

    #[test]
    fn starts_with_compares_components_not_bytes() {
        // The Obsidian folder-note trap: `Projects.md` must NOT be treated as
        // living inside the directory `Projects`.
        let dir = RelPath::new(Path::new("Projects")).unwrap();
        let sibling = RelPath::new(Path::new("Projects.md")).unwrap();
        let child = RelPath::new(Path::new("Projects/plan.md")).unwrap();
        assert!(!sibling.starts_with(&dir));
        assert!(child.starts_with(&dir));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p inkra-vault --lib path`
Expected: FAIL to compile — `cannot find type RelPath in this scope`.

- [ ] **Step 3: Write the implementation**

Above the test module in `crates/vault/src/path.rs`:

```rust
//! A path known to be relative to the vault root, with no `..`.
//!
//! Same discipline as `ByteOffset` and `Utf16Offset` in `inkra-text`: the
//! invariant lives in the type, so no caller has to remember it.

use std::path::{Component, Path};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(Arc<Path>);

impl RelPath {
    /// `None` when the path is absolute, contains `..`, or has a prefix or
    /// root component. Escaping the vault root is not an error to report to
    /// the user, it is a path the vault refuses to name.
    #[must_use]
    pub fn new(path: &Path) -> Option<Self> {
        let escapes = path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        if escapes {
            return None;
        }
        Some(Self(Arc::from(path)))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Indentation depth in the tree: zero for a top-level entry.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.components().count().saturating_sub(1)
    }

    /// Component-wise, never `str::starts_with` — byte prefixes would make
    /// `Projects.md` look like a child of the directory `Projects`.
    #[must_use]
    pub fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }

    /// The name shown in the sidebar. Lossy rather than fallible: a filename
    /// that is not valid UTF-8 should render with replacement characters, not
    /// disappear from the vault.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.0
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
    }
}
```

Add `mod path;` and `pub use path::RelPath;` to `lib.rs` (Task 1 Step 2 already did).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p inkra-vault --lib path`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/src/path.rs crates/vault/src/lib.rs
git commit -m "Add RelPath, a vault path that cannot escape its root"
```

---

### Task 3: The `Fs` trait, its types, and `MemFs`

**Files:**
- Create: `crates/vault/src/fs.rs`, `crates/vault/src/mem_fs.rs`
- Modify: `crates/vault/src/lib.rs`

- [ ] **Step 1: Write the types and the trait**

`crates/vault/src/fs.rs`:

```rust
//! The seam between the vault and the filesystem.
//!
//! Async on purpose. On local disk it buys nothing — `RealFs` performs a
//! blocking read inside the future, because its callers already run under
//! gpui's background executor. What it buys is the shape: a remote backend
//! later is a second `impl Fs` and nothing else moves.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;

use crate::RelPath;

/// What a file *is*, independent of what it is called.
///
/// Paired, not inode alone: an inode is unique only within one filesystem, and
/// nothing stops a vault root spanning a mount point. Zed's `Entry` carries
/// the inode by itself (`vendor/zed-reference/crates/worktree/src/worktree.rs:3963`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mtime(pub SystemTime);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    Note,
}

/// Everything the scan needs, so `read_dir` alone is enough to recurse and no
/// fourth `metadata` method is required.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub kind: Kind,
    pub mtime: Mtime,
    pub identity: FileIdentity,
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub text: String,
    pub mtime: Mtime,
    pub identity: FileIdentity,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: not valid UTF-8")]
    NotUtf8 { path: PathBuf },
    #[error("{path}: outside the vault root")]
    OutsideRoot { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[async_trait]
pub trait Fs: Send + Sync {
    /// Entries directly inside `path`. Not recursive: `Vault::scan` drives the
    /// walk so it can impose a depth limit and refuse to follow symlinks.
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// Reads and normalizes line endings. `\r\n` and a lone `\r` both become
    /// `\n` before the text can reach a `Buffer`, which assumes no `\r`
    /// anywhere.
    async fn load(&self, path: &Path) -> Result<Loaded>;

    /// Truncating write, not atomic — same as Zed
    /// (`vendor/zed-reference/crates/fs/src/fs.rs:1019-1035`). Atomic save is
    /// deferred with its trigger in the spec.
    async fn save(&self, path: &Path, text: &str) -> Result<Mtime>;
}

/// Normalizes CRLF and lone CR to LF.
///
/// `docs/roadmap.md:18` gave CRLF normalization the trigger "file I/O or a
/// paste action, whichever lands first". This is that file I/O.
#[must_use]
pub fn normalize_line_endings(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize_line_endings;

    #[test]
    fn crlf_becomes_lf() {
        assert_eq!(normalize_line_endings("a\r\nb"), "a\nb");
    }

    #[test]
    fn a_lone_cr_becomes_lf() {
        assert_eq!(normalize_line_endings("a\rb"), "a\nb");
    }

    #[test]
    fn text_without_cr_is_unchanged() {
        assert_eq!(normalize_line_endings("a\nb"), "a\nb");
    }

    #[test]
    fn a_trailing_cr_becomes_lf() {
        assert_eq!(normalize_line_endings("a\r"), "a\n");
    }
}
```

`RelPath` is imported but unused here for now; drop the `use crate::RelPath;` line if clippy flags it, and re-add it in Task 9.

- [ ] **Step 2: Write `MemFs`**

`crates/vault/src/mem_fs.rs`:

```rust
//! An in-memory `Fs` for tests.
//!
//! A `BTreeMap` and an injectable error, not a simulated kernel. Zed's
//! `FakeFs` is 2207 lines (`vendor/zed-reference/crates/fs/src/fs.rs:1440-3646`)
//! because it amortizes over hundreds of scanner tests; ours does not.
//!
//! Compiled under clippy's `--all-features`, so it is held to
//! `unwrap_used = "deny"` like everything else.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use crate::fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, VaultError};

#[derive(Default)]
struct State {
    files: BTreeMap<PathBuf, (String, Mtime, FileIdentity)>,
    dirs: BTreeMap<PathBuf, (Mtime, FileIdentity)>,
    next_inode: u64,
    /// Reading any path under this one fails. Models an unreadable directory.
    unreadable: Option<PathBuf>,
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

    /// Panics if the lock is poisoned, which only happens if another test
    /// thread already panicked. `unwrap` is denied, so this is explicit.
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
            state.dirs.insert(dir.to_path_buf(), (Self::now(), identity));
            parent = dir.parent();
        }
        let identity = state
            .files
            .get(&path)
            .map_or_else(|| Self::fresh_identity(&mut state), |(_, _, id)| *id);
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

    /// Moves a file, keeping its identity — which is what a real rename does,
    /// and what the identity proptest exists to check.
    pub fn rename(&self, from: &Path, to: impl Into<PathBuf>) {
        let mut state = self.state();
        if let Some(entry) = state.files.remove(from) {
            state.files.insert(to.into(), entry);
        }
    }

    pub fn make_unreadable(&self, path: impl Into<PathBuf>) {
        self.state().unreadable = Some(path.into());
    }

    #[must_use]
    pub fn read_to_string(&self, path: &Path) -> Option<String> {
        self.state().files.get(path).map(|(text, _, _)| text.clone())
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
```

- [ ] **Step 3: Wire the modules**

`crates/vault/src/lib.rs`:

```rust
//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.

pub mod fs;
mod path;
mod real_fs;

#[cfg(feature = "test-support")]
pub mod mem_fs;

pub use fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, VaultError};
pub use path::RelPath;
pub use real_fs::RealFs;
```

Add `mod real_fs;` and `pub use real_fs::RealFs;` only once Task 4 creates the file — until then, omit both lines so the crate compiles.

- [ ] **Step 4: Verify it builds and the normalizer tests pass**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS, 4 line-ending tests plus the 5 from Task 2.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/src
git commit -m "Add the Fs trait, its types, and an in-memory implementation"
```

---

### Task 4: `RealFs`

**Files:**
- Create: `crates/vault/src/real_fs.rs`
- Modify: `crates/vault/src/lib.rs`
- Test: `crates/vault/tests/real_fs.rs`

- [ ] **Step 1: Write the failing test**

`crates/vault/tests/real_fs.rs`:

```rust
//! `RealFs` against a real temporary directory. Separate from the unit tests
//! because it touches the disk.

use std::fs;
use std::path::PathBuf;

use futures::executor::block_on;
use inkra_vault::{Fs, Kind, RealFs};

/// A temp directory that removes itself. `tempfile` is not a dependency, and
/// this is the only place that would want it.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("inkra-vault-test-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn load_normalizes_crlf_and_reports_identity() {
    let dir = TempDir::new("load");
    let note = dir.0.join("note.md");
    fs::write(&note, "line one\r\nline two\r\n").expect("write");

    let loaded = block_on(RealFs.load(&note)).expect("load");

    assert_eq!(loaded.text, "line one\nline two\n");
    assert!(loaded.identity.inode != 0);
}

#[test]
fn save_writes_lf_and_round_trips() {
    let dir = TempDir::new("save");
    let note = dir.0.join("note.md");

    block_on(RealFs.save(&note, "hello\nworld\n")).expect("save");

    assert_eq!(fs::read_to_string(&note).expect("read"), "hello\nworld\n");
}

#[test]
fn read_dir_separates_directories_from_notes_and_skips_dotfiles() {
    let dir = TempDir::new("read-dir");
    fs::write(dir.0.join("a.md"), "a").expect("write");
    fs::create_dir(dir.0.join("sub")).expect("mkdir");
    fs::write(dir.0.join(".hidden.md"), "h").expect("write");

    let mut entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");
    entries.sort_by(|x, y| x.path.cmp(&y.path));

    assert_eq!(entries.len(), 2, "the dotfile must be skipped");
    assert_eq!(entries[0].path, dir.0.join("a.md"));
    assert_eq!(entries[0].kind, Kind::Note);
    assert_eq!(entries[1].path, dir.0.join("sub"));
    assert_eq!(entries[1].kind, Kind::Dir);
}
```

Integration tests are not subject to the crate's own `unwrap_used` deny the way library code is, but `expect` with a message is used throughout regardless — it names the failure in the test output.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p inkra-vault --test real_fs`
Expected: FAIL to compile — `cannot find RealFs in inkra_vault`.

- [ ] **Step 3: Write `RealFs`**

`crates/vault/src/real_fs.rs`:

```rust
//! The platform filesystem.
//!
//! Every method performs blocking `std::fs` work inside the future, and does
//! not use `smol`. `smol::fs` sits on the `blocking` crate's thread pool, so
//! on local disk it only moves a blocking read onto a different pool — and our
//! callers already run under gpui's `cx.background_spawn`, which is one.
//! Copying Zed that far would add six crates and buy no performance.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use async_trait::async_trait;

use crate::fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, VaultError, normalize_line_endings,
};

pub struct RealFs;

fn io_error(path: &Path, source: std::io::Error) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn identity_of(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

fn mtime_of(path: &Path, metadata: &fs::Metadata) -> Result<Mtime> {
    metadata
        .modified()
        .map(Mtime)
        .map_err(|source| io_error(path, source))
}

#[async_trait]
impl Fs for RealFs {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let reader = fs::read_dir(path).map_err(|source| io_error(path, source))?;

        let mut entries = Vec::new();
        for entry in reader {
            let entry = entry.map_err(|source| io_error(path, source))?;
            let entry_path = entry.path();

            // Dotfiles and dot-directories never belong to a vault.
            let hidden = entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if hidden {
                continue;
            }

            // `symlink_metadata` does not follow the link, which is how the
            // scan avoids symlink loops without needing to detect them.
            let metadata = fs::symlink_metadata(&entry_path)
                .map_err(|source| io_error(&entry_path, source))?;
            if metadata.is_symlink() {
                continue;
            }

            let kind = if metadata.is_dir() {
                Kind::Dir
            } else if entry_path.extension().is_some_and(|ext| ext == "md") {
                Kind::Note
            } else {
                continue;
            };

            entries.push(DirEntry {
                path: entry_path.clone(),
                kind,
                mtime: mtime_of(&entry_path, &metadata)?,
                identity: identity_of(&metadata),
            });
        }
        Ok(entries)
    }

    async fn load(&self, path: &Path) -> Result<Loaded> {
        let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
        let text = String::from_utf8(bytes).map_err(|_| VaultError::NotUtf8 {
            path: path.to_path_buf(),
        })?;
        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;

        Ok(Loaded {
            text: normalize_line_endings(&text),
            mtime: mtime_of(path, &metadata)?,
            identity: identity_of(&metadata),
        })
    }

    async fn save(&self, path: &Path, text: &str) -> Result<Mtime> {
        fs::write(path, text).map_err(|source| io_error(path, source))?;
        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        mtime_of(path, &metadata)
    }
}
```

Add `mod real_fs;` and `pub use real_fs::RealFs;` to `lib.rs`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS, 3 integration tests plus the 9 unit tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault
git commit -m "Add RealFs: blocking std::fs inside the future, no smol"
```

---

### Task 5: `inkra-app` opens a path argument

**Files:**
- Modify: `crates/app/Cargo.toml`, `crates/app/src/main.rs:20-51`

- [ ] **Step 1: Add the dependency**

In `crates/app/Cargo.toml` under `[dependencies]`:

```toml
inkra-vault = { workspace = true }
```

And under `[dev-dependencies]`:

```toml
inkra-vault = { workspace = true, features = ["test-support"] }
```

- [ ] **Step 2: Replace the hardcoded buffer**

`crates/app/src/main.rs`, replacing the literal at `:35-47`. Keep the rest of `main` unchanged.

```rust
use std::path::PathBuf;

use inkra_vault::{Fs, RealFs};

/// The note named on the command line, already loaded.
///
/// Reading the file before the window opens is deliberate: there is nothing to
/// show until it succeeds, and a failure should exit rather than flash an
/// empty window. Once the sidebar exists, loads move to a background task.
fn initial_text() -> Result<String, String> {
    let Some(argument) = std::env::args().nth(1) else {
        // No argument: an empty buffer. The editor still works.
        return Ok(String::new());
    };
    let path = PathBuf::from(argument);
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| loaded.text)
        .map_err(|error| format!("{error}"))
}
```

Then inside `open_window`'s closure, replace the `Buffer::new("…")` literal with:

```rust
let buffer = cx.new(|_| Buffer::new(text.clone()));
```

and, before `cx.open_window`, add:

```rust
let text = match initial_text() {
    Ok(text) => text,
    Err(message) => {
        eprintln!("{message}");
        std::process::exit(1);
    }
};
```

`futures` must be a normal dependency of `app` for `block_on`; add `futures = "0.3"` to `[dependencies]`.

- [ ] **Step 3: Verify by hand — `check.sh` renders nothing**

```bash
printf 'first line\r\nsecond line\r\n' > /tmp/inkra-demo.md
cargo run -p inkra-app -- /tmp/inkra-demo.md
```

Expected: the window shows `first line` and `second line` on two lines, with no stray glyph where the `\r` was. Then:

```bash
cargo run -p inkra-app -- /tmp/does-not-exist.md; echo "exit=$?"
```

Expected: an error naming the path, `exit=1`, no window.

- [ ] **Step 4: Run the gate**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/app Cargo.lock
git commit -m "Open the note named on the command line"
```

---

# Phase 2 — Save

At the end of this phase, editing and pressing `⌘S` writes the file back.

### Task 6: `Buffer::version()` — the id lives on the `Edit`

This is the regression test for critical defect 1. The test in Step 1 **passes** under a per-transaction id and **fails** under no id at all; the second test is the one that fails under a per-transaction id. Write both.

**Files:**
- Modify: `crates/text/src/buffer.rs:17-21`, `:48-58`, `:60-71`, `:158-184`

- [ ] **Step 1: Write the failing tests**

In `crates/text/src/buffer.rs`, inside the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn version_starts_at_zero_and_moves_with_an_edit() {
    let mut buffer = Buffer::new("abc");
    assert_eq!(buffer.version(), 0);
    buffer.insert(ByteOffset(3), "x");
    assert_ne!(buffer.version(), 0);
}

/// The defect that motivated per-edit ids.
///
/// `close` merges an edit that lands within `GROUP_INTERVAL` by extending the
/// previous transaction in place (`buffer.rs:141-148`), so a transaction's
/// identity does not move even though the text changed. With the id on the
/// transaction, this reports "saved" over unsaved text and the edit is lost on
/// quit.
#[test]
fn version_moves_when_an_edit_merges_into_the_previous_transaction() {
    let mut buffer = Buffer::new("abc");
    let t0 = Instant::now();
    buffer.transact_at(t0, |b| b.insert(ByteOffset(3), "x"));
    let saved = buffer.version();

    buffer.transact_at(t0 + Duration::from_millis(120), |b| {
        b.insert(ByteOffset(4), "y");
    });

    assert_eq!(buffer.text(), "abcxy");
    assert_eq!(buffer.undo_stack.len(), 1, "the edits must have merged");
    assert_ne!(
        buffer.version(),
        saved,
        "a merged edit still changes the text, so the version must move"
    );
}

#[test]
fn undo_returns_the_version_to_its_earlier_value() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let after_first = buffer.version();
    buffer.finalize_transaction();
    buffer.transact(|b| b.insert(ByteOffset(4), "y"));

    buffer.undo();

    assert_eq!(buffer.text(), "abcx");
    assert_eq!(buffer.version(), after_first);
}

#[test]
fn redo_restores_the_same_version() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let after_edit = buffer.version();
    buffer.undo();
    buffer.redo();
    assert_eq!(buffer.version(), after_edit);
}

#[test]
fn a_divergent_edit_after_undo_cannot_collide_with_the_history_it_replaced() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let replaced = buffer.version();
    buffer.undo();
    buffer.transact(|b| b.insert(ByteOffset(3), "z"));

    assert_eq!(buffer.text(), "abcz");
    assert_ne!(buffer.version(), replaced);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p inkra-text --lib version`
Expected: FAIL to compile — `no method named version found for struct Buffer`.

- [ ] **Step 3: Implement per-edit ids**

`crates/text/src/buffer.rs`. Add `id` to `Edit` at `:17`:

```rust
struct Edit {
    /// Monotonic across this buffer's whole life, assigned when the edit is
    /// applied. `Buffer::version` is the id of the last applied edit.
    ///
    /// On the edit, not on the `Transaction`: `close` merges by extending
    /// `previous.edits` in place, so a transaction's identity does not move
    /// when the text does.
    id: u64,
    at: ByteOffset,
    removed: String,
    inserted: String,
}
```

`inverted()` must carry the id through so redo restores the same version:

```rust
fn inverted(&self) -> Self {
    Self {
        id: self.id,
        at: self.at,
        removed: self.inserted.clone(),
        inserted: self.removed.clone(),
    }
}
```

Add the counter to `Buffer` at `:48` and to `new` at `:62`:

```rust
    next_edit_id: u64,
```

```rust
            next_edit_id: 1,
```

In `replace`, at `:179`, assign the id:

```rust
        let id = self.next_edit_id;
        self.next_edit_id += 1;
        self.record(Edit {
            id,
            at: range.start,
            removed,
            inserted: text.to_string(),
        });
```

Add the accessor after `is_empty` at `:86`:

```rust
    /// Identifies the text's position in its own history.
    ///
    /// Two buffers are not comparable through this; it answers one question,
    /// for one buffer: has the text moved since a version I previously held.
    /// `app` stores the version it last wrote to disk and compares.
    ///
    /// Zed reaches the same property with a `clock::Global` version vector and
    /// a memoized `has_unsaved_edits`
    /// (`vendor/zed-reference/crates/language/src/buffer.rs:139`, `:2493`). It
    /// counts operations, which is the granularity adopted here. Ours is a
    /// single `u64` because we have one writer.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.undo_stack
            .last()
            .and_then(|transaction| transaction.edits.last())
            .map_or(0, |edit| edit.id)
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p inkra-text`
Expected: PASS, including the existing suite and the proptest.

- [ ] **Step 5: Extend the buffer proptest**

In `crates/text/tests/buffer_proptest.rs`, inside the existing edit-sequence strategy, after each applied operation record `(version, text)` and assert at the end:

```rust
// Equal versions imply equal text. The converse does not hold: undoing and
// retyping the same characters reaches the same text at a new version.
for (i, (version_i, text_i)) in observations.iter().enumerate() {
    for (version_j, text_j) in observations.iter().skip(i + 1) {
        if version_i == version_j {
            prop_assert_eq!(text_i, text_j);
        }
    }
}
```

- [ ] **Step 6: Run the gate and commit**

Run: `./scripts/check.sh`

```bash
git add crates/text
git commit -m "Add Buffer::version, with the id on the edit not the transaction"
```

---

### Task 7: `NoteStore` and the `Save` action

**Files:**
- Create: `crates/app/src/vault.rs`
- Modify: `crates/app/src/main.rs`, `crates/app/src/shell.rs`, `crates/app/src/keymap.rs`

- [ ] **Step 1: Write `NoteStore`**

`crates/app/src/vault.rs`:

```rust
//! The binding between files and buffers.
//!
//! Holds **strong** `Entity<Buffer>` handles. Weak ones would drop an unsaved
//! buffer the moment the editor switched away from it, losing the text and the
//! undo history with no prompt — see critical defect 2 in the design spec.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Context, Entity, Task, actions, prelude::*};
use inkra_text::Buffer;
use inkra_vault::{EntryId, Fs, Mtime, Vault};

actions!(vault, [Save]);

/// What is known about one open note's relationship to its file.
struct OpenNote {
    buffer: Entity<Buffer>,
    path: PathBuf,
    /// `Buffer::version` as of the last successful write.
    saved_version: u64,
    saved_mtime: Mtime,
}

pub(crate) struct NoteStore {
    fs: Arc<dyn Fs>,
    vault: Option<Entity<Vault>>,
    open: HashMap<EntryId, OpenNote>,
    current: Option<EntryId>,
}

impl NoteStore {
    pub(crate) fn new(fs: Arc<dyn Fs>, vault: Option<Entity<Vault>>) -> Self {
        Self {
            fs,
            vault,
            open: HashMap::new(),
            current: None,
        }
    }

    pub(crate) fn vault(&self) -> Option<&Entity<Vault>> {
        self.vault.as_ref()
    }

    pub(crate) fn current(&self) -> Option<EntryId> {
        self.current
    }

    /// True when the note's buffer has moved past what was last written.
    pub(crate) fn is_modified(&self, id: EntryId, cx: &Context<Self>) -> bool {
        self.open
            .get(&id)
            .is_some_and(|note| note.buffer.read(cx).version() != note.saved_version)
    }

    /// Writes the current note. A no-op when nothing has changed since the
    /// last write.
    ///
    /// The version is captured **with** the text, before the task is spawned.
    /// `⌘S` is async, so keystrokes can land while the write is in flight;
    /// recording the captured version rather than a later one leaves those
    /// edits correctly dirty.
    ///
    /// `finalize_transaction` runs here, in the action handler, and never in an
    /// observer callback: a second lease of one entity panics
    /// (`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:231`).
    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(id) = self.current else {
            return Task::ready(());
        };
        let Some(note) = self.open.get(&id) else {
            return Task::ready(());
        };

        let (text, version) = {
            let mut buffer = note.buffer.as_mut(cx);
            buffer.finalize_transaction();
            (buffer.text().to_owned(), buffer.version())
        };
        if version == note.saved_version {
            return Task::ready(());
        }

        let fs = Arc::clone(&self.fs);
        let path = note.path.clone();
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn(async move { fs.save(&path, &text).await })
                .await;
            let mtime = match written {
                Ok(mtime) => mtime,
                Err(error) => {
                    eprintln!("save failed: {error}");
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(note) = this.open.get_mut(&id) {
                    note.saved_version = version;
                    note.saved_mtime = mtime;
                }
                cx.notify();
            });
        })
    }
}
```

`Vault` and `EntryId` do not exist until **Task 8**. Until then, replace `Option<Entity<Vault>>` with `Option<()>` and define a placeholder id in this file, deleting both when Task 8 lands:

```rust
// Removed in Task 8, which introduces the real one in inkra-vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EntryId(pub u32);
```

The alternative — writing the store after the model — would leave `⌘S` untested for two more tasks.

- [ ] **Step 2: Have `AppShell` own the store and handle the action**

`crates/app/src/shell.rs`. Add the field and the handler; keep `Focusable` as it is for now.

```rust
pub(crate) struct AppShell {
    editor: Entity<EditorView>,
    notes: Entity<NoteStore>,
}

impl AppShell {
    pub(crate) fn new(editor: Entity<EditorView>, notes: Entity<NoteStore>) -> Self {
        Self { editor, notes }
    }

    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes
            .update(cx, |notes, cx| notes.save_current(cx))
            .detach();
    }
}
```

In `render`, wrap the outer `div()` with the handler and a key context:

```rust
div()
    .key_context("AppShell")
    .on_action(cx.listener(Self::save))
    .flex()
    .flex_row()
    .size_full()
```

- [ ] **Step 3: Bind the key**

`crates/app/src/keymap.rs`, inside `key_bindings()`'s vector:

```rust
// Bound at the AppShell context, not EditorView's: the sidebar is
// focusable, and a binding scoped to the editor would do nothing while a
// note row has focus.
KeyBinding::new("cmd-s", Save, Some("AppShell")),
```

Import `crate::vault::Save`.

- [ ] **Step 4: Write the keystroke test**

CLAUDE.md requires a `simulate_keystrokes` test per binding: a handler called directly proves nothing about the `actions!` mapping or the binding itself.

`crates/app/src/vault.rs`, in a `#[cfg(test)] mod tests`:

```rust
#[gpui::test]
fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (shell, window, mut visual) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    visual.simulate_keystrokes("cmd-s");
    // The write runs on a background task; without this the assertion races it.
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("Xbefore")
    );
    let _ = (shell, window);
}
```

`open_shell` is a new helper in the same module. It must open the window through `AppShell`, because neither `with_editor` nor `dispatching` (`crates/app/src/editor/tests.rs:20`, `:35`) does — see the precedent and its reasoning at `crates/app/src/shell.rs:69-73`.

```rust
#[cfg(test)]
fn open_shell(
    cx: &mut TestAppContext,
    fs: Arc<dyn Fs>,
) -> (Entity<AppShell>, AnyWindowHandle, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(crate::keymap::key_bindings()));
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("before")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
    let notes = cx.update(|cx| {
        cx.new(|_| {
            let mut store = NoteStore::new(fs, None);
            store.adopt_for_test(buffer, PathBuf::from("/vault/a.md"));
            store
        })
    });
    let window = cx.add_window({
        let editor = editor.clone();
        let notes = notes.clone();
        move |_, _| AppShell::new(editor, notes)
    });
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("focus the shell");
    let visual = VisualTestContext::from_window(window.into(), cx);
    let shell = window.root(cx).expect("root view");
    (shell, window.into(), visual)
}
```

`adopt_for_test` is a `#[cfg(test)]` method on `NoteStore` that inserts an `OpenNote` with `saved_version: 0` and sets `current`. It exists because Task 12 is what wires real opening; without it this binding would go untested until then.

- [ ] **Step 5: Run and commit**

Run: `./scripts/check.sh`

```bash
git add crates/app
git commit -m "Save the current note with cmd-s"
```

---

# Phase 3 — Scan, and a list you can click

### Task 8: `Entry`, `EntryId`, and the scan

**Files:**
- Create: `crates/vault/src/vault.rs`
- Modify: `crates/vault/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/vault/src/vault.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::path::Path;

    use futures::executor::block_on;

    use super::*;
    use crate::mem_fs::MemFs;

    fn scan(fs: &MemFs) -> Vault {
        block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
    }

    #[test]
    fn the_scan_finds_notes_in_subdirectories() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/sub/b.md", "b");

        let vault = scan(&fs);

        let names: Vec<_> = vault.entries().map(|e| e.path.display_name()).collect();
        assert!(names.contains(&"a".to_owned()));
        assert!(names.contains(&"b".to_owned()));
    }

    #[test]
    fn an_empty_directory_is_kept_as_an_entry() {
        let fs = MemFs::new();
        fs.insert_dir("/vault/empty");

        let vault = scan(&fs);

        assert_eq!(vault.entries().count(), 1);
        assert_eq!(vault.entries().next().map(|e| e.kind), Some(Kind::Dir));
    }

    #[test]
    fn an_unreadable_subdirectory_does_not_lose_the_vault() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_dir("/vault/locked");
        fs.make_unreadable("/vault/locked");

        let vault = scan(&fs);

        assert!(vault.entries().any(|e| e.path.display_name() == "a"));
    }

    #[test]
    fn a_rename_keeps_the_entry_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        let before = scan(&fs);
        let id = before.entries().next().expect("one entry").id;

        fs.rename(Path::new("/vault/a.md"), "/vault/b.md");
        let after = scan(&fs);

        let entry = after.entry(id).expect("the id must survive a rename");
        assert_eq!(entry.path.display_name(), "b");
    }

    #[test]
    fn a_file_recreated_at_the_same_path_gets_a_new_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "first");
        let before = scan(&fs);
        let old = before.entries().next().expect("one entry").id;

        fs.remove(Path::new("/vault/a.md"));
        fs.insert_file("/vault/a.md", "second");
        let after = scan(&fs);

        let new = after.entries().next().expect("one entry").id;
        assert_ne!(old, new, "a different file must not inherit the id");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p inkra-vault --all-features --lib vault`
Expected: FAIL to compile — `cannot find type Vault`.

- [ ] **Step 3: Write the model and the scan**

Above the tests in `crates/vault/src/vault.rs`:

```rust
//! The vault model: what notes exist, and what each one is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::RelPath;
use crate::fs::{FileIdentity, Fs, Kind, Mtime, Result};

/// No real vault approaches this. It exists so a pathological tree cannot
/// exhaust the walk.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(u32);

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: EntryId,
    pub path: RelPath,
    pub kind: Kind,
    pub mtime: Mtime,
}

pub struct Vault {
    root: PathBuf,
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    by_path: HashMap<RelPath, EntryId>,
    next_id: u32,
}

impl Vault {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Never returns the `Vec`. Swapping the container for a `SumTree` the day
    /// a profile asks for one must stay internal.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The absolute path to an entry, for handing to `Fs`.
    #[must_use]
    pub fn absolute(&self, id: EntryId) -> Option<PathBuf> {
        self.entry(id).map(|entry| self.root.join(entry.path.as_path()))
    }

    /// Walks the tree from `root`, keeping `*.md` and every directory.
    ///
    /// Iterative rather than recursive: an async recursive function needs
    /// boxing at every level, and an explicit worklist gives the depth limit
    /// for free.
    pub async fn scan(fs: &dyn Fs, root: &Path) -> Result<Self> {
        let mut vault = Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            by_identity: HashMap::new(),
            by_path: HashMap::new(),
            next_id: 1,
        };
        vault.rescan(fs).await?;
        Ok(vault)
    }

    /// Re-walks the tree, preserving the id of every file whose identity is
    /// unchanged. Ids are keyed by `(device, inode)`, so a rename keeps its id
    /// and a recreated file takes a new one.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let mut found: Vec<Entry> = Vec::new();
        let mut next_by_identity = HashMap::new();
        let mut worklist = vec![(self.root.clone(), 0usize)];

        while let Some((dir, depth)) = worklist.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            // One unreadable folder must not lose the vault.
            let Ok(children) = fs.read_dir(&dir).await else {
                continue;
            };

            for child in children {
                let Ok(relative) = child.path.strip_prefix(&self.root) else {
                    continue;
                };
                let Some(path) = RelPath::new(relative) else {
                    continue;
                };

                let id = match self.by_identity.get(&child.identity) {
                    Some(existing) => *existing,
                    None => {
                        let id = EntryId(self.next_id);
                        self.next_id += 1;
                        id
                    }
                };
                // Hard links collapse: the first path wins, the second is
                // dropped. A note vault does not use them.
                if next_by_identity.insert(child.identity, id).is_some() {
                    continue;
                }

                if child.kind == Kind::Dir {
                    worklist.push((child.path.clone(), depth + 1));
                }
                found.push(Entry {
                    id,
                    path,
                    kind: child.kind,
                    mtime: child.mtime,
                });
            }
        }

        self.by_path = found
            .iter()
            .map(|entry| (entry.path.clone(), entry.id))
            .collect();
        self.by_identity = next_by_identity;
        self.entries = found;
        Ok(())
    }
}
```

`Vault::entry` above is written for clarity, not speed; Task 12 replaces its body with a `HashMap<EntryId, usize>` lookup once the sidebar makes it hot. Ordering comes in Task 10 — until then `entries()` yields walk order, and the tests above do not depend on it.

Export from `lib.rs`:

```rust
mod vault;
pub use vault::{Entry, EntryId, Vault};
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS, 5 new tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault
git commit -m "Scan a vault, with entry identity from (device, inode)"
```

---

### Task 9: The identity proptest

This is the property the whole design rests on, and the one the first draft of the spec could not have passed.

**Files:**
- Create: `crates/vault/tests/identity_proptest.rs`

- [ ] **Step 1: Write the proptest**

```rust
//! Entry ids survive an arbitrary sequence of vault mutations.
//!
//! Path-keyed ids fail this: a rename is a delete plus an insert, so the id
//! changes exactly when it must not. See the design spec's "Identity" section.

use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::{EntryId, Vault};
use inkra_vault::mem_fs::MemFs;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Add(String),
    Remove(usize),
    Rename(usize, String),
    /// `Notes.md` → `notes.md`: one file on a case-insensitive filesystem.
    RecaseName(usize),
}

fn name() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "Alpha".to_owned(),
        "Beta".to_owned(),
        "Gamma".to_owned(),
        "Delta".to_owned(),
    ])
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        name().prop_map(Op::Add),
        (0usize..8).prop_map(Op::Remove),
        (0usize..8, name()).prop_map(|(i, n)| Op::Rename(i, n)),
        (0usize..8).prop_map(Op::RecaseName),
    ]
}

proptest! {
    #[test]
    fn identity_survives_arbitrary_mutation(ops in prop::collection::vec(op(), 1..25)) {
        let fs = MemFs::new();
        fs.insert_file("/vault/Alpha.md", "seed");

        let mut vault = block_on(Vault::scan(&fs, Path::new("/vault"))).expect("scan");

        // What each surviving file is expected to still be called, by id.
        let mut expected: Vec<(EntryId, PathBuf)> = vault
            .entries()
            .map(|e| (e.id, e.path.as_path().to_path_buf()))
            .collect();

        for operation in ops {
            match operation {
                Op::Add(n) => {
                    let path = format!("/vault/{n}.md");
                    if fs.read_to_string(Path::new(&path)).is_none() {
                        fs.insert_file(path, "new");
                    }
                }
                Op::Remove(i) => {
                    if let Some((_, path)) = expected.get(i) {
                        fs.remove(&PathBuf::from("/vault").join(path));
                    }
                }
                Op::Rename(i, n) => {
                    if let Some((id, path)) = expected.get(i).cloned() {
                        let from = PathBuf::from("/vault").join(&path);
                        let to = format!("/vault/{n}.md");
                        if fs.read_to_string(Path::new(&to)).is_none() {
                            fs.rename(&from, &to);
                            expected[i] = (id, PathBuf::from(format!("{n}.md")));
                        }
                    }
                }
                Op::RecaseName(i) => {
                    if let Some((id, path)) = expected.get(i).cloned() {
                        let from = PathBuf::from("/vault").join(&path);
                        let lowered = path.to_string_lossy().to_lowercase();
                        if fs.read_to_string(Path::new(&from)).is_some() {
                            fs.rename(&from, PathBuf::from("/vault").join(&lowered));
                            expected[i] = (id, PathBuf::from(lowered));
                        }
                    }
                }
            }

            block_on(vault.rescan(&fs)).expect("rescan");

            // Every file still on disk keeps the id it had.
            for (id, path) in &expected {
                let absolute = PathBuf::from("/vault").join(path);
                if fs.read_to_string(&absolute).is_some() {
                    let entry = vault.entry(*id);
                    prop_assert!(
                        entry.is_some(),
                        "id {id:?} vanished while {path:?} is still on disk"
                    );
                    if let Some(entry) = entry {
                        prop_assert_eq!(entry.path.as_path(), path.as_path());
                    }
                }
            }
            expected.retain(|(_, p)| {
                fs.read_to_string(&PathBuf::from("/vault").join(p)).is_some()
            });
        }
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p inkra-vault --all-features --test identity_proptest`
Expected: PASS. If it fails, the failure is real — do not weaken the property. Commit `.proptest-regressions` as CLAUDE.md requires.

- [ ] **Step 3: Commit**

```bash
git add crates/vault/tests crates/vault/.proptest-regressions 2>/dev/null || git add crates/vault/tests
git commit -m "Prove entry ids survive renames, recasing and recreation"
```

---

### Task 10: The comparator and the contiguity invariant

**Files:**
- Modify: `crates/vault/src/vault.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/vault/src/vault.rs`'s test module:

```rust
fn ordered_paths(fs: &MemFs) -> Vec<String> {
    scan(fs)
        .entries()
        .map(|e| e.path.as_path().to_string_lossy().into_owned())
        .collect()
}

/// Byte order puts `a/b.md` between directory `a/b` and its own children,
/// because `.` is 0x2E and `/` is 0x2F. The Obsidian folder-note pattern hits
/// this immediately.
#[test]
fn a_folder_note_does_not_split_its_folder_from_its_children() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects.md", "n");

    assert_eq!(
        ordered_paths(&fs),
        vec!["Projects", "Projects/plan.md", "Projects.md"]
    );
}

/// Sorting by `(kind, path)` puts every directory before every file, so
/// `a/z.md` would render indented under `c`.
#[test]
fn a_directorys_children_follow_it_rather_than_all_other_directories() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a/z.md", "z");
    fs.insert_file("/vault/c/y.md", "y");
    fs.insert_file("/vault/b.md", "b");

    assert_eq!(
        ordered_paths(&fs),
        vec!["a", "a/z.md", "c", "c/y.md", "b.md"]
    );
}

/// The invariant the sidebar's collapse-skip rests on, asserted directly.
#[test]
fn every_directorys_descendants_are_contiguous() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects/deep/x.md", "x");
    fs.insert_file("/vault/Projects.md", "n");
    fs.insert_file("/vault/a.md", "a");

    let vault = scan(&fs);
    let entries: Vec<_> = vault.entries().collect();

    for (i, entry) in entries.iter().enumerate() {
        if entry.kind != Kind::Dir {
            continue;
        }
        let descendants = entries
            .iter()
            .filter(|c| c.path != entry.path && c.path.starts_with(&entry.path))
            .count();
        for offset in 1..=descendants {
            assert!(
                entries[i + offset].path.starts_with(&entry.path),
                "{:?} is not contiguous: index {} breaks the run",
                entry.path.as_path(),
                i + offset
            );
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p inkra-vault --all-features --lib vault`
Expected: FAIL — `entries()` still yields walk order, so the first assertion mismatches.

- [ ] **Step 3: Write the comparator**

In `crates/vault/src/vault.rs`:

```rust
/// Orders entries so that a directory is immediately followed by its whole
/// subtree, which is what lets the sidebar collapse a folder with a forward
/// skip instead of a tree rebuild.
///
/// Neither obvious encoding works. Plain path bytes put `a/b.md` between `a/b`
/// and `a/b/c.md`, because `.` (0x2E) sorts below `/` (0x2F). Sorting by
/// `(kind, path)` puts every directory before every file. So: compare
/// component by component, and at the first difference prefer the one whose
/// component *at that position* is a directory — which is not the same as the
/// entry being a directory.
fn compare(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a_components: Vec<_> = a.path.as_path().components().collect();
    let b_components: Vec<_> = b.path.as_path().components().collect();

    for (i, (x, y)) in a_components.iter().zip(b_components.iter()).enumerate() {
        if x == y {
            continue;
        }
        // A component is a directory when it is not the last one, or when the
        // entry itself is a directory.
        let x_is_dir = i + 1 < a_components.len() || a.kind == Kind::Dir;
        let y_is_dir = i + 1 < b_components.len() || b.kind == Kind::Dir;
        return match (x_is_dir, y_is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => x.cmp(y),
        };
    }
    // One is an ancestor of the other: the ancestor comes first.
    a_components.len().cmp(&b_components.len())
}
```

At the end of `rescan`, before assigning `self.entries`:

```rust
        found.sort_by(compare);
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS, including the three ordering tests and the identity proptest.

- [ ] **Step 5: Commit**

```bash
git add crates/vault
git commit -m "Order entries so a folder is followed by its own subtree"
```

---

### Task 11: `EditorView::open_buffer`, the third gate

This is the regression test for critical defect 3. CLAUDE.md says `EditorView` has exactly two gates; Task 15 updates that rule.

**Files:**
- Modify: `crates/app/src/editor/mod.rs:65-83`
- Test: `crates/app/src/editor/tests.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/app/src/editor/tests.rs`:

```rust
/// The defect: `_buffer_changed` is a `Subscription` that unsubscribes on
/// drop (`editor/mod.rs:58-62`). Swapping the buffer without re-observing
/// leaves the view watching the closed note — edits to the new one repaint
/// nothing, with no compiler warning.
#[gpui::test]
fn editing_after_a_swap_still_notifies(cx: &mut TestAppContext) {
    let first = cx.update(|cx| cx.new(|_| Buffer::new("first")));
    let second = cx.update(|cx| cx.new(|_| Buffer::new("second")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(first, cx)));

    let notified = Arc::new(AtomicUsize::new(0));
    let _guard = cx.update(|cx| {
        let notified = Arc::clone(&notified);
        cx.observe(&editor, move |_, _| {
            notified.fetch_add(1, Ordering::SeqCst);
        })
    });

    editor.update(cx, |view, cx| view.open_buffer(second.clone(), cx));
    let before = notified.load(Ordering::SeqCst);
    second.update(cx, |buffer, cx| {
        buffer.insert(ByteOffset(0), "x");
        cx.notify();
    });
    cx.run_until_parked();

    assert!(
        notified.load(Ordering::SeqCst) > before,
        "the view must observe the buffer it now holds"
    );
}

#[gpui::test]
fn a_swap_resets_the_caret_and_any_ime_composition(cx: &mut TestAppContext) {
    let first = cx.update(|cx| cx.new(|_| Buffer::new("first")));
    let second = cx.update(|cx| cx.new(|_| Buffer::new("a much longer second note")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(first, cx)));

    editor.update(cx, |view, cx| {
        view.state.set_marked_range(Some(ByteOffset(0)..ByteOffset(2)));
        view.open_buffer(second, cx);
    });

    editor.update(cx, |view, _| {
        assert_eq!(view.state.cursor(), ByteOffset(0), "a note opens at the top");
        assert_eq!(
            view.state.marked_range(),
            None,
            "an in-flight composition must not survive into another document"
        );
        assert!(view.last_layout.is_none(), "stale geometry must be cleared");
    });
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p inkra-app open_buffer`
Expected: FAIL to compile — `no method named open_buffer`.

- [ ] **Step 3: Implement the gate**

In `crates/app/src/editor/mod.rs`, after `new`:

```rust
    /// The third gate: replaces the buffer this view edits.
    ///
    /// `edit_buffer` hands out `&mut Buffer` and `move_cursor` a `&Buffer`;
    /// neither can rebind `self.buffer`, and going through them would push the
    /// new note's text onto the *previous* note's undo stack — `⌘Z` would then
    /// restore one file into another.
    ///
    /// Every field reset below is a defect if skipped, so they are exhaustive
    /// rather than minimal.
    pub(crate) fn open_buffer(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        // Rebuilt, not kept: the old `Subscription` unsubscribes when it drops,
        // and keeping it would observe the closed note forever.
        self._buffer_changed = cx.observe(&buffer, |_, _, cx| cx.notify());
        self.buffer = buffer;

        // Offset 0, unlike `new`, which seats the caret at `end`. Opening a
        // note starts at the top; resuming the session does not.
        self.state = EditorState::new(Selection::at(ByteOffset(0)));
        self.is_selecting = false;

        // `last_layout` describes the previous note's text. `reveal_line`
        // early-returns on `None` (`:144-146`), so a stale non-`None` layout
        // would scroll the new note by the old note's geometry.
        self.last_layout = None;
        self.last_bounds = None;
        self.last_text_top = None;
        self.scroll_handle = ScrollHandle::new();

        cx.notify();
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p inkra-app`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/editor
git commit -m "Add EditorView::open_buffer, a third gate that re-observes"
```

---

### Task 12: The sidebar, and opening a note by clicking it

This carries the regression test for critical defect 2.

**Files:**
- Create: `crates/app/src/sidebar.rs`
- Modify: `crates/app/src/shell.rs`, `crates/app/src/vault.rs`, `crates/app/src/main.rs`

- [ ] **Step 1: Write the failing test**

In `crates/app/src/vault.rs`'s test module:

```rust
/// The defect: holding buffers weakly meant switching notes dropped the last
/// strong handle, so returning reloaded from disk and the unsaved text and the
/// whole undo history were gone, with no prompt.
#[gpui::test]
fn switching_away_and_back_preserves_unsaved_text(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");

    let (store, a, b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    store.update(cx, |store, cx| {
        store.open_note(a, cx).detach();
    });
    cx.run_until_parked();
    store.update(cx, |store, cx| {
        if let Some(buffer) = store.buffer_for(a) {
            buffer.update(cx, |buffer, _| buffer.insert(ByteOffset(0), "UNSAVED "));
        }
    });

    store.update(cx, |store, cx| store.open_note(b, cx).detach());
    cx.run_until_parked();
    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    cx.run_until_parked();

    store.update(cx, |store, cx| {
        let buffer = store.buffer_for(a).expect("the buffer must still exist");
        assert_eq!(buffer.read(cx).text(), "UNSAVED alpha");
        assert!(buffer.read(cx).version() != 0, "the undo history survived too");
    });
}
```

`open_two_notes` is a helper in the same module:

```rust
#[cfg(test)]
fn open_two_notes(
    cx: &mut TestAppContext,
    fs: Arc<dyn Fs>,
) -> (Entity<NoteStore>, EntryId, EntryId) {
    let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
        .expect("scan the test vault");
    // Resolved by name, never by index: the comparator decides the order and
    // this test must not silently depend on it.
    let id_of = |name: &str| {
        scanned
            .entries()
            .find(|entry| entry.path.display_name() == name)
            .map(|entry| entry.id)
            .expect("entry exists")
    };
    let (a, b) = (id_of("a"), id_of("b"));
    let vault = cx.update(|cx| cx.new(|_| scanned));
    let store = cx.update(|cx| cx.new(|_| NoteStore::new(fs, Some(vault))));
    (store, a, b)
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p inkra-app switching_away`
Expected: FAIL to compile — `no method named open_note`.

- [ ] **Step 3: Add `open_note` to `NoteStore`**

```rust
    pub(crate) fn buffer_for(&self, id: EntryId) -> Option<&Entity<Buffer>> {
        self.open.get(&id).map(|note| &note.buffer)
    }

    /// Loads a note, or returns the buffer already open for it.
    ///
    /// The map holds a **strong** handle, so an unsaved buffer survives being
    /// switched away from. There is no close action in this slice, so buffers
    /// live for the session; eviction is deferred with its trigger.
    pub(crate) fn open_note(
        &mut self,
        id: EntryId,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Buffer>>> {
        if let Some(note) = self.open.get(&id) {
            self.current = Some(id);
            let buffer = note.buffer.clone();
            cx.notify();
            return Task::ready(Some(buffer));
        }

        let Some(vault) = self.vault.clone() else {
            return Task::ready(None);
        };
        let Some(path) = vault.read(cx).absolute(id) else {
            return Task::ready(None);
        };
        let fs = Arc::clone(&self.fs);

        cx.spawn(async move |this, cx| {
            let for_task = path.clone();
            let loaded = cx
                .background_spawn(async move { fs.load(&for_task).await })
                .await
                .ok()?;
            this.update(cx, |this, cx| {
                let buffer = cx.new(|_| Buffer::new(loaded.text));
                this.open.insert(
                    id,
                    OpenNote {
                        buffer: buffer.clone(),
                        path,
                        saved_version: 0,
                        saved_mtime: loaded.mtime,
                    },
                );
                this.current = Some(id);
                cx.notify();
                buffer
            })
            .ok()
        })
    }
```

- [ ] **Step 4: Write the sidebar**

`crates/app/src/sidebar.rs`:

```rust
//! The vault tree.
//!
//! Rendered as a linear walk over path-ordered entries, never as a per-frame
//! tree rebuild: `inkra_vault`'s comparator guarantees a directory's
//! descendants are contiguous, so collapsing one is a forward skip.

use std::collections::HashSet;

use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb,
    uniform_list,
};
use inkra_vault::{EntryId, Kind};

use crate::vault::NoteStore;

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    expanded: HashSet<EntryId>,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
}

#[derive(Clone)]
struct Row {
    id: EntryId,
    depth: usize,
    kind: Kind,
    name: String,
}

impl Sidebar {
    pub(crate) fn new(notes: Entity<NoteStore>, cx: &mut Context<Self>) -> Self {
        let mut sidebar = Self {
            notes,
            expanded: HashSet::new(),
            visible: Vec::new(),
        };
        sidebar.rebuild(cx);
        sidebar
    }

    /// One forward pass. A collapsed directory's descendants are skipped by
    /// advancing while `RelPath::starts_with` holds — component-wise, so the
    /// sibling `Projects.md` is never swallowed by the folder `Projects`.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            return;
        };
        let vault = vault.read(cx);
        let entries: Vec<_> = vault.entries().cloned().collect();

        let mut i = 0;
        while i < entries.len() {
            let entry = &entries[i];
            self.visible.push(Row {
                id: entry.id,
                depth: entry.path.depth(),
                kind: entry.kind,
                name: entry.path.display_name(),
            });
            if entry.kind == Kind::Dir && !self.expanded.contains(&entry.id) {
                i += 1;
                while i < entries.len() && entries[i].path.starts_with(&entry.path) {
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
        cx.notify();
    }

    fn toggle(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(cx);
    }
}

impl Render for Sidebar {
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.visible.clone();
        // Computed here, not inside the row closure: that closure is
        // `'static + Fn` and cannot borrow `cx`.
        let modified: Vec<bool> = self
            .notes
            .update(cx, |notes, cx| {
                rows.iter().map(|row| notes.is_modified(row.id, cx)).collect()
            });

        uniform_list("vault-tree", rows.len(), {
            let rows = rows.clone();
            move |range, _window, _cx| {
                range
                    .map(|ix| {
                        let row = &rows[ix];
                        let dot = if modified.get(ix).copied().unwrap_or(false) {
                            " •"
                        } else {
                            ""
                        };
                        div()
                            .flex()
                            .flex_row()
                            .h(px(22.))
                            .pl(px(8. + 12. * row.depth as f32))
                            // Required, not cosmetic: item height is measured
                            // from item 0 at MaxContent
                            // (uniform_list.rs:658-680), so a long filename
                            // would wrap at paint and overflow its row.
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_color(rgb(0xd0d0d0))
                            .child(format!("{}{dot}", row.name))
                    })
                    .collect()
            }
        })
        // Required: ListSizingBehavior defaults to Auto
        // (gpui/elements/list.rs:192-193), whose branch requests layout with no
        // children (uniform_list.rs:315-318). Without this the list resolves to
        // zero height inside the sidebar's fixed-width div and paints nothing.
        .size_full()
    }
}
```

`Sidebar` also holds `editor: Entity<EditorView>`, passed by `AppShell`, so a resolved buffer can reach the third gate. Add the click handler as a method and attach it to each row with `.on_mouse_down(MouseButton::Left, cx.listener(...))`:

```rust
    /// A directory toggles; a note loads and is handed to
    /// `EditorView::open_buffer`, which is the only path that may rebind the
    /// view's buffer.
    fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        let editor = self.editor.clone();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(row.id, cx));
        cx.spawn(async move |_, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx));
        })
        .detach();
    }
```

Inside the `uniform_list` closure each row becomes, wrapping the `div()` already shown:

```rust
.on_mouse_down(
    MouseButton::Left,
    cx.listener({
        let row = row.clone();
        move |sidebar, _event, _window, cx| sidebar.activate(&row, cx)
    }),
)
```

The closure passed to `uniform_list` is `'static + Fn(Range<usize>, &mut Window, &mut App)`, so it receives an `&mut App` rather than a `&mut Context<Sidebar>`. Build the listeners by making the closure capture `cx.entity()` and calling `entity.update(cx, ...)` inside, rather than `cx.listener`, which is only available on a `Context<Self>`.

- [ ] **Step 5: Host it in `AppShell`**

Replace the grey placeholder at `crates/app/src/shell.rs:46`:

```rust
            .child(
                div()
                    .flex_none()
                    .w(px(180.))
                    .h_full()
                    .bg(rgb(0x181818))
                    .child(self.sidebar.clone()),
            )
```

- [ ] **Step 6: Run and verify by hand**

Run: `./scripts/check.sh`

```bash
mkdir -p /tmp/inkra-vault-demo/Projects
printf 'alpha\n' > /tmp/inkra-vault-demo/a.md
printf 'note beside a folder\n' > /tmp/inkra-vault-demo/Projects.md
printf 'plan\n' > /tmp/inkra-vault-demo/Projects/plan.md
printf 'a very long file name that must truncate rather than wrap\n' \
  > /tmp/inkra-vault-demo/a-very-long-file-name-that-must-truncate-rather-than-wrap.md
cargo run -p inkra-app -- /tmp/inkra-vault-demo
```

Expected, and each is one of the two `uniform_list` traps or the comparator:
- The tree is **not blank**.
- `Projects` appears with `plan` under it, and `Projects.md` as a **sibling below**, not inside.
- The long name **truncates on one line**, and rows do not overlap.
- Clicking a note loads it; editing shows a `•`; `⌘S` clears it.
- Switching notes with unsaved text and switching back keeps the text.

- [ ] **Step 7: Commit**

```bash
git add crates/app
git commit -m "Show the vault as a tree and open a note by clicking it"
```

---

# Phase 4 — Documentation

### Task 13: Update the documents this change invalidates

Not a follow-up. Each is wrong the moment Task 12 ships.

**Files:**
- Modify: `CLAUDE.md`, `docs/roadmap.md`, `docs/superpowers/specs/2026-09-20-modular-architecture-design.md:245`

- [ ] **Step 1: CLAUDE.md**

- The "exactly two gates" rule becomes three, naming `open_buffer` and why it cannot go through the other two.
- Add to "Don't recreate": `EditorView::open_buffer`, `NoteStore` (`app/src/vault.rs`), `Vault` / `Entry` / `EntryId` (`vault/src/vault.rs`), `RelPath` (`vault/src/path.rs`), `Fs` / `MemFs` / `RealFs` (`vault/src/fs.rs`), `Buffer::version` (`text/src/buffer.rs`).
- The architecture section records that `inkra-vault` exists, is gpui-free, and depends on nothing in the workspace.
- Add: entry identity is `(device, inode)`, never a path.

- [ ] **Step 2: `docs/roadmap.md`**

Remove the CRLF row from the editor table — this slice fired its trigger — and add a "Vault" section carrying the Deferred table from the design spec verbatim, including the new CRLF round-trip row.

- [ ] **Step 3: The modular architecture doc**

At `:245`, note that the `inkra-vault` → `text` + anchors row is superseded by
`docs/superpowers/specs/2026-09-20-inkra-vault-design.md`: anchors are needed
by diff-based reload, which lives above the vault.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs
git commit -m "Record the vault in the rules it changes"
```

---

## Definition of done

- [ ] `./scripts/check.sh` green.
- [ ] The three critical regressions each have a named test: Task 6 (merged edit moves the version), Task 11 (editing after a swap notifies), Task 12 (switching away and back preserves unsaved text).
- [ ] The manual render pass in Task 12 Step 6 done, with the folder-note case and the long filename checked by eye — `check.sh` renders nothing.
- [ ] `git diff --stat main` reviewed hunk by hunk, with debug prints and dead code removed.
