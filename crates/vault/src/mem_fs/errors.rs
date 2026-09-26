use std::io;
use std::path::Path;

use crate::fs::{Mtime, Stamp, VaultError};

pub(super) fn io(path: &Path, kind: io::ErrorKind) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source: io::Error::from(kind),
    }
}

pub(super) fn stamp_of(text: &str, mtime: Mtime) -> Stamp {
    Stamp {
        mtime,
        size: text.len() as u64,
    }
}
