use super::conflict_path;
use crate::fs::Mtime;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn at(seconds: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

#[test]
fn it_sits_beside_the_note_and_keeps_the_md_extension() {
    let path = conflict_path(Path::new("/vault/sub/Note.md"), at(1_790_001_125), |_| {
        false
    });
    assert_eq!(
        path,
        PathBuf::from("/vault/sub/Note (conflict 2026-09-21 14-32-05Z).md")
    );
}

#[test]
fn a_taken_name_gets_a_counter() {
    let taken =
        |candidate: &Path| candidate == Path::new("/vault/Note (conflict 1970-01-01 00-00-10Z).md");
    assert_eq!(
        conflict_path(Path::new("/vault/Note.md"), at(10), taken),
        PathBuf::from("/vault/Note (conflict 1970-01-01 00-00-10Z) 2.md")
    );
}

#[test]
fn a_note_without_an_extension_still_gets_one() {
    let path = conflict_path(Path::new("/tmp/notes"), at(10), |_| false);
    assert_eq!(
        path,
        PathBuf::from("/tmp/notes (conflict 1970-01-01 00-00-10Z).md")
    );
}
