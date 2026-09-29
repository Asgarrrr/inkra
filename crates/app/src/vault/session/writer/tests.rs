use std::path::{Path, PathBuf};
use std::sync::Arc;

use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Mtime, Stamp};

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
