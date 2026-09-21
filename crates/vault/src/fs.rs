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

/// What a note looked like on disk, as cheaply as a `stat` can tell us.
///
/// Compared with `!=`, never with an ordering. A sync client that preserves a
/// file's original modified date hands back an mtime *older* than the one we
/// wrote, and `>` calls that unchanged and overwrites it. Zed compares with
/// ordering, through a method it named `bad_is_greater_than`
/// (`vendor/zed-reference/crates/fs/src/fs.rs:422-425`), on a type that refuses
/// to derive `Ord` precisely because the comparison is unsound.
///
/// `size` is the second term because mtime granularity is one second on HFS+
/// and coarser on some network mounts, so two writes can share a timestamp. It
/// does not close the gap — same granule and same length still compares equal —
/// and the design spec records why a content hash is not worth its cost.
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

/// Everything the scan needs, so there is no fourth `metadata` method.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub kind: Kind,
    pub mtime: Mtime,
    pub identity: FileIdentity,
}

/// `stamp` is the expectation the next write of this note carries.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub text: String,
    pub stamp: Stamp,
    pub identity: FileIdentity,
}

/// What a write produced. Symmetric with `Loaded`, and for one reason:
/// `RealFs::save` renames a temp file over the destination, so the file's
/// `(device, inode)` after a save is not the one before it. A caller that keys
/// anything by identity — `Vault` keys every `EntryId` — has to be told.
#[derive(Debug, Clone)]
pub struct Saved {
    pub stamp: Stamp,
    pub identity: FileIdentity,
    /// Where the bytes this write replaced were preserved, when the file had
    /// moved out from under the caller's expectation. `None` is the ordinary
    /// case: the file was exactly what the caller thought it was.
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
    /// Direct children only: `Vault::scan` drives the walk, which is what lets
    /// it impose a depth limit. Filtering is this method's obligation, not the
    /// scan's — yield only directories and `*.md`, never a dotfile, never a
    /// symlink, which is how the walk avoids link loops without detecting them.
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// Normalizes line endings: `Buffer` assumes no `\r` anywhere.
    async fn load(&self, path: &Path) -> Result<Loaded>;

    /// Replaces the file rather than truncating it, so a crash mid-write cannot
    /// leave a half-written note. Zed does the opposite for buffer saves
    /// (`vendor/zed-reference/crates/fs/src/fs.rs:1019-1033` is `File::create`)
    /// and reserves `atomic_write` (`:974`) for its own config files; it can
    /// afford to, because its autosave defaults to off and the user picks the
    /// moment. Ours writes unprompted.
    ///
    /// `expected` is what the caller believes is on disk. When the file says
    /// otherwise, its current bytes are preserved beside it before this write
    /// lands, and the path they went to comes back in `Saved::preserved`.
    /// Required rather than optional: an `Option` would read as "write anyway",
    /// which is the clobber this parameter exists to remove.
    ///
    /// Returns the identity the file carries **afterwards**, which a rename
    /// changes. See `Saved`.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved>;
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
