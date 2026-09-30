//! The session file: plain data, no gpui (restore-tabs spec, R1–R2).

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

/// The tabs of one vault, in bar order, and the active one among them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) version: u32,
    /// The vault's absolute root: a file whose root differs is not this vault's.
    pub(crate) root: PathBuf,
    pub(crate) active: usize,
    pub(crate) tabs: Vec<SavedTab>,
}

/// `note` is vault-relative, `None` for an empty tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedTab {
    pub(crate) note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) view: Option<SavedView>,
}

/// A view in byte offsets, without its goal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedView {
    pub(crate) tail: usize,
    pub(crate) head: usize,
    pub(crate) top: usize,
    pub(crate) below: f32,
}

/// Why a session file restores nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rejected {
    Corrupt,
    Version,
    Root,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Corrupt => "corrupt",
            Self::Version => "other version",
            Self::Root => "other vault",
        })
    }
}

impl Session {
    pub(crate) fn new(root: PathBuf, active: usize, tabs: Vec<SavedTab>) -> Self {
        Self {
            version: VERSION,
            root,
            active,
            tabs,
        }
    }

    /// Fails only on a root that is not UTF-8.
    pub(crate) fn encode(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub(crate) fn decode(text: &str, root: &Path) -> Result<Self, Rejected> {
        let session: Self = serde_json::from_str(text).map_err(|_| Rejected::Corrupt)?;
        if session.version != VERSION {
            return Err(Rejected::Version);
        }
        if session.root != root {
            return Err(Rejected::Root);
        }
        Ok(session)
    }
}

/// FNV-1a 64, written out: `DefaultHasher`'s output may change between Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// Where the session of the vault at `root`, absolute, lives under `home` (R1).
pub(crate) fn session_file(home: &Path, root: &Path) -> PathBuf {
    let key = fnv1a(root.as_os_str().as_encoded_bytes());
    home.join("Library/Application Support/Inkra/sessions")
        .join(format!("{key:016x}.json"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
