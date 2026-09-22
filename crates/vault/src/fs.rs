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
/// Compared with `!=`, never with an ordering: a sync client that preserves a
/// file's original modified date hands back an mtime *older* than the one we
/// wrote, and `>` would call that unchanged and overwrite it. Zed compares
/// with ordering via `bad_is_greater_than`
/// (`vendor/zed-reference/crates/fs/src/fs.rs:423-424`) on a type whose own
/// doc says not to derive `Ord` (`:334`).
///
/// `size` is the second term because mtime granularity is one second on HFS+
/// and coarser on some network mounts, so two writes can share a timestamp —
/// it does not close the gap (same granule, same length still compares equal);
/// see the design spec on why a content hash is not worth it.
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

/// `stamp` is the expectation the next write of this note carries. `identity`
/// has no reader: only `Saved::identity` does, because only a write moves it.
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
    /// (`vendor/zed-reference/crates/fs/src/fs.rs:1019-1033`), reserving
    /// `atomic_write` (`:974`) for its own config files since its autosave
    /// defaults to off and the user picks the moment; ours writes unprompted.
    ///
    /// No write may destroy bytes it did not put there. `expected` is what the
    /// caller believes is on disk; when the file disagrees, its current bytes
    /// are preserved beside it **before** this write lands, so both copies
    /// exist together and no crash point loses either. The path they went to
    /// comes back in `Saved::preserved`. `expected` is required, not optional:
    /// an `Option` would read as "write anyway", the clobber this exists to
    /// prevent. A second `Fs` backend owes the same ordering.
    ///
    /// Returns the identity the file carries **afterwards**, which a rename
    /// changes. See `Saved`.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved>;

    /// Creates an empty note or a directory. Fails if anything already exists
    /// at `path`, rather than taking it over.
    ///
    /// One method with a `Kind` rather than two: the two differ only in which
    /// node type lands, and the caller already carries the `Kind`. Returns the
    /// new identity, which is how a caller finds the entry the next scan mints
    /// for it — by inode, never by path, so a scan racing the create cannot
    /// hand back a different file that happens to share the name.
    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity>;

    /// Moves `from` to `to`, and never replaces a *different* file at `to`.
    ///
    /// Returns nothing because nothing moves: no implementation of this may
    /// change the inode, so a caller holding `from`'s identity still holds
    /// `to`'s. `Vault::apply` keys every `EntryId` on that, which is what makes
    /// a rename keep its entry.
    ///
    /// `to` naming the *same* file as `from` is a rename, not a collision: on a
    /// case-insensitive or normalization-insensitive volume that is how the
    /// stored spelling of a name is changed, and it is the only way a user can
    /// fix the capitalisation of their own note.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Moves a note to the platform trash, from where the user can recover it.
    ///
    /// Notes only. A directory would take its whole subtree with it, which
    /// needs a prompt that counts what it destroys and the closing of every
    /// buffer inside it; both are deferred.
    async fn trash(&self, path: &Path) -> Result<()>;
}

/// CRLF and lone CR to LF. `Buffer` assumes LF.
///
/// Public because both file I/O and a paste need it: `Fs::load` is one door
/// into a buffer and a paste is the other, so `app` calls this on clipboard
/// text too. It belongs to text arriving from outside rather than to the
/// filesystem, but cannot move to `inkra-text` — this crate depends on nothing
/// in the workspace, by design — without duplicating the four tests below.
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

    /// Two writes inside one mtime granule differ only here; dropping it would
    /// leave the whole workspace green.
    #[test]
    fn a_stamp_differs_when_only_the_size_does() {
        use super::{Mtime, Stamp};
        use std::time::{Duration, SystemTime};

        let at = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
        assert_ne!(Stamp { mtime: at, size: 1 }, Stamp { mtime: at, size: 2 });
    }

    /// The only test in the suite with an mtime moving backwards; every other
    /// conflict test would still pass if `!=` were relaxed to `>`. See `Stamp`.
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
