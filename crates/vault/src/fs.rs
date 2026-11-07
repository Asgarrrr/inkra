//! Async buys nothing on local disk — `RealFs` blocks inside the future — but
//! it buys the shape: a remote backend later is a second `impl Fs`.

use std::any::Any;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;

use crate::note_file_name;
use crate::watch::Watch;

/// Paired, not inode alone: an inode is unique only within one filesystem, and
/// a vault root can span a mount point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mtime(pub SystemTime);

/// What a note looked like on disk. Compare with `!=`, never an ordering: a
/// sync client can hand back an mtime *older* than what we wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: Mtime,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    Note,
}

impl Kind {
    /// The file name `typed` gets as this kind: a note's via `note_file_name`, a folder's trimmed.
    #[must_use]
    pub fn file_name(self, typed: &str) -> String {
        match self {
            Self::Note => note_file_name(typed),
            Self::Dir => typed.trim().to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub kind: Kind,
    pub mtime: Mtime,
    pub size: u64,
    pub identity: FileIdentity,
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub text: String,
    pub stamp: Stamp,
    pub identity: FileIdentity,
}

/// What a write produced. `identity` can differ from `Loaded::identity`: a
/// save renames a temp file over the destination, so the inode moves.
#[derive(Debug, Clone)]
pub struct Saved {
    pub stamp: Stamp,
    pub identity: FileIdentity,
    /// `None` unless the file had moved out from under the caller's expectation.
    pub preserved: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: not valid UTF-8")]
    NotUtf8 { path: PathBuf },
    /// The save preserved the note's bytes, then failed to write: `stamp` is the kept bytes'.
    #[error("{source} (the text on disk was kept at {preserved})")]
    Unwritten {
        preserved: PathBuf,
        stamp: Stamp,
        source: Box<VaultError>,
    },
}

impl VaultError {
    /// The failed call's kind; an `Unwritten` save answers its cause's.
    #[must_use]
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::Unwritten { source, .. } => source.io_kind(),
            Self::NotUtf8 { .. } => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;

/// Dropping it stops the watch and ends its stream.
pub struct Watcher {
    _held: Box<dyn Any + Send + Sync>,
}

impl Watcher {
    pub(crate) fn new(held: impl Any + Send + Sync) -> Self {
        Self {
            _held: Box::new(held),
        }
    }
}

#[async_trait]
pub trait Fs: Send + Sync {
    /// Direct children only, filtering out dotfiles and symlinks itself so
    /// the walk avoids link loops without detecting them.
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// `dir`'s direct children that are regular files, any extension, no dotfiles or symlinks,
    /// sorted by path; a missing `dir` is an `Io` `NotFound` error.
    async fn list_files(&self, dir: &Path) -> Result<Vec<PathBuf>>;

    /// Fails unless `path` is a folder this process may list; lists nothing.
    async fn check_dir(&self, path: &Path) -> Result<()>;

    /// Normalizes line endings: `Buffer` assumes no `\r` anywhere.
    async fn load(&self, path: &Path) -> Result<Loaded>;

    /// The bytes as stored. A file over `max_len` is refused from its size before any read.
    async fn read_bytes(&self, path: &Path, max_len: u64) -> Result<Vec<u8>>;

    /// Replaces the file rather than truncating it. If `expected` disagrees
    /// with what's on disk, preserves the current bytes before overwriting.
    /// Fails when `path`'s folder is missing.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved>;

    /// Creates every missing folder above `path`, as `mkdir -p`; succeeds when they exist. Not
    /// part of `save`: only the caller knows the folder should come back (a note gone from disk),
    /// and a present note whose folder moved away must fail, not be written beside its copy.
    async fn create_parents(&self, path: &Path) -> Result<()>;

    /// Atomic, creating missing folders; no conflict check, so for the app's own state, never a note.
    async fn replace(&self, path: &Path, text: &str) -> Result<()>;

    /// As `replace`, with bytes that need not be UTF-8.
    async fn replace_bytes(&self, path: &Path, bytes: &[u8]) -> Result<()>;

    /// Removes a file; one already missing is `Ok`.
    async fn remove_file(&self, path: &Path) -> Result<()>;

    /// Creates an empty note or directory; fails if `path` already exists.
    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity>;

    /// Moves `from` to `to`, never replacing a *different* file at `to`, and
    /// never changing the inode.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Moves a note to the platform trash. Notes only; directories deferred.
    async fn trash(&self, path: &Path) -> Result<()>;

    /// Watches `root` and everything below it; forwards every event, filtering none.
    ///
    /// # Errors
    ///
    /// When `root` cannot be resolved or the platform refuses the watch.
    fn watch(&self, root: &Path) -> Result<Watch>;
}

/// CRLF and lone CR to LF. `Buffer` assumes LF. Public because both
/// `Fs::load` and a clipboard paste need it.
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
mod tests;
