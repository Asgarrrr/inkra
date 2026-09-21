//! Async buys nothing on local disk — `RealFs` blocks inside the future — but
//! it buys the shape: a remote backend later is a second `impl Fs`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;

/// Paired, not inode alone: an inode is unique only within one filesystem, and
/// nothing stops a vault root spanning a mount point. Zed's `Entry` carries the
/// inode by itself (`vendor/zed-reference/crates/worktree/src/worktree.rs:3963`).
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

/// Everything the scan needs, so there is no fourth `metadata` method.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub kind: Kind,
    pub mtime: Mtime,
    pub identity: FileIdentity,
}

/// Only `text` has a caller today. `mtime` is what conflict detection will seed
/// `saved_mtime` from (Deferred in
/// `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`).
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
    /// Direct children only: `Vault::scan` drives the walk, which is what lets
    /// it impose a depth limit. Filtering is this method's obligation, not the
    /// scan's — yield only directories and `*.md`, never a dotfile, never a
    /// symlink, which is how the walk avoids link loops without detecting them.
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// Normalizes line endings: `Buffer` assumes no `\r` anywhere.
    async fn load(&self, path: &Path) -> Result<Loaded>;

    /// Truncating, not atomic — same as Zed (`crates/fs/src/fs.rs:1019-1035`);
    /// atomic save is deferred with its trigger in the spec.
    async fn save(&self, path: &Path, text: &str) -> Result<Mtime>;
}

/// CRLF and lone CR to LF. `docs/roadmap.md:18` gave this the trigger "file
/// I/O or a paste action, whichever lands first".
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
