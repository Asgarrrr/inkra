//! `RealFs` against a real temporary directory. Separate from the unit tests
//! because it touches the disk.

// `[lints] workspace = true` in Cargo.toml applies to every target of the
// crate, integration tests included, so `expect_used` is denied here too. A
// failed setup call in a test must abort with a message naming what broke, not
// be threaded through a `Result` that hides it.
#![allow(clippy::expect_used)]

use std::fs;
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
