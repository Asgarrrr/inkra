use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Mtime, Stamp, VaultError};

fn writer() -> Arc<super::Writer> {
    Arc::new(super::Writer {
        state: futures::lock::Mutex::new(super::WriterState {
            written: 0,
            // Deliberately mismatched: the first write is expected to
            // find a conflict and preserve.
            stamp: Stamp {
                mtime: Mtime(std::time::SystemTime::UNIX_EPOCH),
                size: 0,
            },
        }),
    })
}

/// Driven directly rather than raced: a `#[gpui::test]` can't model a
/// background thread already inside `rename()`.
#[test]
fn a_later_write_wins_when_it_lands_second() {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");
    let writer = writer();
    let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

    let older = super::write_through(
        Arc::clone(&writer),
        Arc::clone(&dyn_fs),
        PathBuf::from("/vault/a.md"),
        "older".into(),
        1,
        super::Folder::MustExist,
    );
    let newer = super::write_through(
        writer,
        dyn_fs,
        PathBuf::from("/vault/a.md"),
        "newer".into(),
        2,
        super::Folder::MustExist,
    );
    assert!(futures::executor::block_on(older).is_some_and(|r| r.is_ok()));
    assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("newer")
    );
}

#[test]
fn a_later_write_wins_when_it_lands_first() {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");
    let writer = writer();
    let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

    let newer = super::write_through(
        Arc::clone(&writer),
        Arc::clone(&dyn_fs),
        PathBuf::from("/vault/a.md"),
        "newer".into(),
        2,
        super::Folder::MustExist,
    );
    let older = super::write_through(
        writer,
        dyn_fs,
        PathBuf::from("/vault/a.md"),
        "older".into(),
        1,
        super::Folder::MustExist,
    );
    assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

    assert!(
        futures::executor::block_on(older).is_none(),
        "the stale write must be skipped, not merely reported"
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("newer"),
        "the older write must not have reached the file at all"
    );
}

/// The bytes a failed write preserved sit beside the note, so a retry over them destroys nothing.
#[test]
fn a_failure_after_preserving_adopts_the_kept_stamp() {
    let fs = Arc::new(MemFs::new());
    let path = Path::new("/vault/a.md");
    fs.insert_file(path, "theirs");
    fs.fail_next_saves_after_preserve(path, 1, ErrorKind::Interrupted);
    let writer = writer();

    let written = futures::executor::block_on(super::write_through(
        Arc::clone(&writer),
        Arc::clone(&fs) as Arc<dyn Fs>,
        path.to_path_buf(),
        "mine".into(),
        1,
        super::Folder::MustExist,
    ));

    assert!(matches!(written, Some(Err(VaultError::Unwritten { .. }))));
    let on_disk = futures::executor::block_on(fs.load(path))
        .ok()
        .map(|loaded| loaded.stamp);
    assert_eq!(
        Some(futures::executor::block_on(writer.expected())),
        on_disk
    );
}

#[test]
fn a_plain_failure_keeps_the_expected_stamp() {
    let fs = Arc::new(MemFs::new());
    let path = Path::new("/vault/a.md");
    fs.insert_file(path, "theirs");
    fs.fail_next_saves(path, 1, ErrorKind::Interrupted);
    let writer = writer();
    let before = futures::executor::block_on(writer.expected());

    let written = futures::executor::block_on(super::write_through(
        Arc::clone(&writer),
        Arc::clone(&fs) as Arc<dyn Fs>,
        path.to_path_buf(),
        "mine".into(),
        1,
        super::Folder::MustExist,
    ));

    assert!(matches!(written, Some(Err(VaultError::Io { .. }))));
    assert_eq!(futures::executor::block_on(writer.expected()), before);
}
