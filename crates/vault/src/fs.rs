//! Async buys nothing on local disk — `RealFs` blocks inside the future — but
//! it buys the shape: a remote backend later is a second `impl Fs`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;

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
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[async_trait]
pub trait Fs: Send + Sync {
    /// Direct children only, filtering out dotfiles and symlinks itself so
    /// the walk avoids link loops without detecting them.
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// Normalizes line endings: `Buffer` assumes no `\r` anywhere.
    async fn load(&self, path: &Path) -> Result<Loaded>;

    /// Replaces the file rather than truncating it. If `expected` disagrees
    /// with what's on disk, preserves the current bytes before overwriting.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved>;

    /// Creates an empty note or directory; fails if `path` already exists.
    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity>;

    /// Moves `from` to `to`, never replacing a *different* file at `to`, and
    /// never changing the inode.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Moves a note to the platform trash. Notes only; directories deferred.
    async fn trash(&self, path: &Path) -> Result<()>;
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
mod tests {
    use super::normalize_line_endings;

    #[test]
    fn a_stamp_differs_when_only_the_size_does() {
        use super::{Mtime, Stamp};
        use std::time::{Duration, SystemTime};

        let at = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
        assert_ne!(Stamp { mtime: at, size: 1 }, Stamp { mtime: at, size: 2 });
    }

    /// Every other conflict test would still pass if `!=` were relaxed to `>`.
    #[test]
    fn a_stamp_differs_when_the_mtime_moves_backwards() {
        use super::{Mtime, Stamp};
        use std::time::{Duration, SystemTime};

        let newer = Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(20)),
            size: 4,
        };
        let older = Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10)),
            size: 4,
        };
        assert_ne!(newer, older);
    }

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
