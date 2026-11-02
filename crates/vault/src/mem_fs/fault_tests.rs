use std::io::ErrorKind;
use std::path::Path;

use futures::executor::block_on;

use super::MemFs;
use crate::fs::{Fs, VaultError};

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

#[test]
fn fail_next_saves_fails_that_many_saves_then_writes() {
    let fs = MemFs::new();
    let path = Path::new("/vault/a.md");
    fs.insert_file(path, "alpha");
    let stamp = block_on(fs.load(path)).expect("loads").stamp;

    fs.fail_next_saves(path, 2, ErrorKind::Interrupted);
    for _ in 0..2 {
        let error = block_on(fs.save(path, "beta", stamp)).expect_err("an injected failure");
        assert!(
            matches!(&error, VaultError::Io { source, .. } if source.kind() == ErrorKind::Interrupted),
            "{error}"
        );
    }
    assert_eq!(fs.read_to_string(path).as_deref(), Some("alpha"));

    block_on(fs.save(path, "beta", stamp)).expect("the third save writes");
    assert_eq!(fs.read_to_string(path).as_deref(), Some("beta"));
}

#[test]
fn fail_next_saves_after_preserve_keeps_one_copy_and_reports_unwritten() {
    let fs = MemFs::new();
    let path = Path::new("/vault/a.md");
    fs.insert_file(path, "alpha");
    let stale = block_on(fs.load(path)).expect("loads").stamp;
    fs.insert_file(path, "theirs");

    fs.fail_next_saves_after_preserve(path, 2, ErrorKind::Interrupted);
    let error = block_on(fs.save(path, "mine", stale)).expect_err("fails after preserving");
    let VaultError::Unwritten {
        preserved,
        stamp,
        source,
    } = error
    else {
        panic!("not Unwritten: {error}");
    };
    assert!(
        matches!(&*source, VaultError::Io { source, .. } if source.kind() == ErrorKind::Interrupted),
        "{source}"
    );
    assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
    assert_eq!(fs.read_to_string(path).as_deref(), Some("theirs"));

    let saved = block_on(fs.save(path, "mine", stamp)).expect("the kept stamp preserves nothing");
    assert_eq!(saved.preserved, None);
    assert_eq!(fs.read_to_string(path).as_deref(), Some("mine"));
    let copies = block_on(fs.read_dir(Path::new("/vault")))
        .expect("lists")
        .into_iter()
        .filter(|entry| entry.path.to_string_lossy().contains("conflict"))
        .count();
    assert_eq!(copies, 1);
}
