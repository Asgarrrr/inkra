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

/// Only `text` has a caller in `app` today. The other two come from the same
/// `stat` and are what conflict detection seeds `saved_mtime` from — the
/// Deferred table in `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`
/// names `Fs::load` as that source. `identity` is also the only assertion that
/// `RealFs` reports a real inode (`crates/vault/tests/real_fs.rs`).
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
