//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.
//!
//! No gpui, no workspace dependency — `Fs::load` returns a `String` and `app`
//! builds the `Buffer`. See
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.

mod civil;
mod conflict;
// Private, and re-exported whole below: a `pub mod` gave every type two public
// paths — `inkra_vault::Fs` and `inkra_vault::fs::Fs` — and no caller outside
// this crate ever used the second.
mod fs;
mod path;
mod real_fs;
mod vault;

#[cfg(feature = "test-support")]
pub mod mem_fs;

pub use fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError,
    normalize_line_endings,
};
pub use path::RelPath;
pub use real_fs::RealFs;
pub use vault::{Entry, EntryId, Vault};
