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

fn units(path: &Path) -> usize {
    path.file_name()
        .expect("a name")
        .to_string_lossy()
        .encode_utf16()
        .count()
}

#[test]
fn a_long_stem_is_cut_so_the_name_fits() {
    let note = PathBuf::from(format!("/vault/{}.md", "a".repeat(252)));
    let path = conflict_path(&note, at(10), |_| false);
    assert_eq!(units(&path), 255);
    assert!(
        path.to_string_lossy()
            .ends_with(" (conflict 1970-01-01 00-00-10Z).md")
    );
}

#[test]
fn a_long_stem_pays_for_the_counter_too() {
    let note = PathBuf::from(format!("/vault/{}.md", "a".repeat(252)));
    let first = conflict_path(&note, at(10), |_| false);
    let second = conflict_path(&note, at(10), |candidate| candidate == first);
    assert!(units(&second) <= 255);
    assert!(
        second
            .to_string_lossy()
            .ends_with(" (conflict 1970-01-01 00-00-10Z) 2.md")
    );
}
