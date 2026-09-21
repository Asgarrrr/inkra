//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.
//!
//! No gpui, no workspace dependency — `Fs::load` returns a `String` and `app`
//! builds the `Buffer`. See
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.

mod civil;
mod conflict;
pub mod fs;
mod path;
mod real_fs;
mod vault;

#[cfg(feature = "test-support")]
pub mod mem_fs;

pub use fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError};
pub use path::RelPath;
pub use real_fs::RealFs;
pub use vault::{Entry, EntryId, Vault};
