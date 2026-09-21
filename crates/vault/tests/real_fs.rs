//! `RealFs` against a real temporary directory. Separate from the unit tests
//! because it touches the disk.

// `[lints] workspace = true` in Cargo.toml applies to every target of the
// crate, integration tests included, so `expect_used` is denied here too. A
// failed setup call in a test must abort with a message naming what broke, not
// be threaded through a `Result` that hides it.
#![allow(clippy::expect_used)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;

use futures::executor::block_on;
use inkra_vault::{Fs, Kind, RealFs};

/// A temp directory that removes itself. `tempfile` is not a dependency, and
/// this is the only place that would want it.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("inkra-vault-test-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn load_normalizes_crlf_and_reports_identity() {
    let dir = TempDir::new("load");
    let note = dir.0.join("note.md");
    fs::write(&note, "line one\r\nline two\r\n").expect("write");

    let loaded = block_on(RealFs.load(&note)).expect("load");

    assert_eq!(loaded.text, "line one\nline two\n");
    assert_ne!(loaded.identity.inode, 0);
}

#[test]
fn save_writes_lf_and_round_trips() {
    let dir = TempDir::new("save");
    let note = dir.0.join("note.md");

    block_on(RealFs.save(&note, "hello\nworld\n")).expect("save");

    assert_eq!(fs::read_to_string(&note).expect("read"), "hello\nworld\n");
}

/// The write replaces the file rather than truncating it, which is what makes
/// a crash mid-save unable to leave a partial note — and what forces
/// `Fs::save` to report the identity afterwards.
#[test]
fn save_replaces_the_file_and_reports_the_new_identity() {
    let dir = TempDir::new("replace");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let before = fs::metadata(&note).expect("stat").ino();

    let saved = block_on(RealFs.save(&note, "after")).expect("save");

    assert_ne!(saved.identity.inode, before, "a rename gives a new inode");
    assert_eq!(
        saved.identity.inode,
        fs::metadata(&note).expect("stat").ino(),
        "the reported identity is the one the file now carries"
    );
    assert_eq!(fs::read_to_string(&note).expect("read"), "after");
}

/// A fresh temp file carries the process umask. Without the restore step the
/// save silently narrows the note — invisible, because the default mode
/// usually matches by accident, hence the explicit `chmod` here.
#[test]
fn save_preserves_the_file_mode() {
    let dir = TempDir::new("mode");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    fs::set_permissions(&note, fs::Permissions::from_mode(0o640)).expect("chmod");

    block_on(RealFs.save(&note, "after")).expect("save");

    let mode = fs::metadata(&note).expect("stat").permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
}

/// `?` unlinks nothing on its way out, so the temp file needs an explicit
/// owner. A leaked one is invisible in the vault — `read_dir` filters it on
/// extension — which is exactly why a test has to look for it directly.
#[test]
fn save_leaves_no_temp_file_behind() {
    let dir = TempDir::new("temp");
    let note = dir.0.join("note.md");

    block_on(RealFs.save(&note, "text")).expect("save");

    let names: Vec<_> = fs::read_dir(&dir.0)
        .expect("read_dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, vec!["note.md"]);
}

/// Replacing a hard-linked note would split a pair that `Vault::rescan`
/// collapses into one entry, surfacing the second name as a new row holding
/// stale text. So a linked note is written in place instead.
#[test]
fn save_of_a_hard_linked_note_keeps_one_inode() {
    let dir = TempDir::new("links");
    let note = dir.0.join("note.md");
    let other = dir.0.join("other.md");
    fs::write(&note, "before").expect("write");
    fs::hard_link(&note, &other).expect("link");
    let before = fs::metadata(&note).expect("stat").ino();

    let saved = block_on(RealFs.save(&note, "after")).expect("save");

    assert_eq!(saved.identity.inode, before, "the inode must not move");
    assert_eq!(
        fs::read_to_string(&other).expect("read"),
        "after",
        "the other name sees the write, as it did before"
    );
}

#[test]
fn read_dir_separates_directories_from_notes_and_skips_dotfiles() {
    let dir = TempDir::new("read-dir");
    fs::write(dir.0.join("a.md"), "a").expect("write");
    fs::create_dir(dir.0.join("sub")).expect("mkdir");
    fs::write(dir.0.join(".hidden.md"), "h").expect("write");

    let mut entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");
    entries.sort_by(|x, y| x.path.cmp(&y.path));

    assert_eq!(entries.len(), 2, "the dotfile must be skipped");
    assert_eq!(entries[0].path, dir.0.join("a.md"));
    assert_eq!(entries[0].kind, Kind::Note);
    assert_eq!(entries[1].path, dir.0.join("sub"));
    assert_eq!(entries[1].kind, Kind::Dir);
}

/// The two filters the dotfile test does not reach. Both are load-bearing and
/// neither has a visible symptom when removed: a non-`.md` file would become a
/// note, and a symlink would let the walk leave the vault root or loop.
#[test]
fn read_dir_skips_other_extensions_and_symlinks() {
    let dir = TempDir::new("filters");
    fs::write(dir.0.join("a.md"), "a").expect("write");
    fs::write(dir.0.join("notes.txt"), "t").expect("write");
    // Deliberately outside the vault: following this link is exactly the
    // escape `symlink_metadata` refuses.
    std::os::unix::fs::symlink(std::env::temp_dir(), dir.0.join("escape.md"))
        .expect("create symlink");

    let entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");

    let paths: Vec<_> = entries.iter().map(|entry| entry.path.clone()).collect();
    assert_eq!(paths, vec![dir.0.join("a.md")]);
}
