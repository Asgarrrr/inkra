use std::path::Path;

use futures::executor::block_on;

use super::MemFs;
use crate::fs::Fs;

#[test]
fn block_after_rename_parks_after_the_file_moved() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "alpha");

    let gate = fs.block_after_rename("/vault/a.md");
    let mut rename = Box::pin(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/z.md")));
    assert!(block_on(async { futures::poll!(rename.as_mut()) }).is_pending());
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
    assert_eq!(
        fs.read_to_string(Path::new("/vault/z.md")).as_deref(),
        Some("alpha"),
        "the file is at its new path while the rename is parked"
    );

    gate.release();
    block_on(rename).expect("the rename finishes once released");
}
