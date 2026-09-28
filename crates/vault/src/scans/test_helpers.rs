//! Fixtures shared by the scan session's tests.

use std::path::Path;

use futures::executor::block_on;

use super::{Claims, Scan, Scanned, Scans, Swapped};
use crate::Serial;
use crate::mem_fs::MemFs;
use crate::vault::Vault;

pub(super) const ROOT: &str = "/vault";

pub(super) fn vault(files: &[&str]) -> MemFs {
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for file in files {
        fs.insert_file(Path::new(ROOT).join(file), "");
    }
    fs
}

pub(super) fn walked(scan: Scan, fs: &MemFs) -> Scanned {
    block_on(scan.walk(fs, &Serial))
}

/// Walks `scan` and swaps it in, with no open note.
pub(super) fn run(
    scans: &mut Scans,
    live: &mut Vault,
    scan: Scan,
    fs: &MemFs,
) -> (Swapped, Option<Scan>) {
    scans.swap(live, walked(scan, fs), &Claims::default())
}
