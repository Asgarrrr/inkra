//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.

mod civil;
mod conflict;
// Private and re-exported below, so callers only see `inkra_vault::Fs`.
mod fs;
mod jobs;
mod path;
mod real_fs;
mod scans;
mod temp;
mod vault;
mod watch;

#[cfg(feature = "test-support")]
pub mod mem_fs;

pub use fs::{
    DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError, Watcher,
    normalize_line_endings,
};
pub use jobs::{Job, Jobs, Serial};
pub use path::{NameProblem, RelPath, check_name, note_file_name};
pub use real_fs::RealFs;
pub use scans::{Claims, Listed, Scan, Scanned, Scans, Swapped};
pub use vault::{Entry, EntryId, Reach, Vault, Walk};
pub use watch::{Changed, PathEvent, Pending, Watch};
