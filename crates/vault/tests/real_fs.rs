//! `RealFs` against a real temporary directory. Separate from the unit tests
//! because it touches the disk.

// `[lints] workspace = true` in Cargo.toml applies to every target of the
// crate, integration tests included, so `expect_used` is denied here too. A
// failed setup call in a test must abort with a message naming what broke, not
// be threaded through a `Result` that hides it.
#![allow(clippy::expect_used)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use futures::executor::block_on;
use inkra_vault::{Fs, Kind, Mtime, RealFs, Stamp};

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

/// What the file currently says, for a save that expects to find it unchanged.
fn stamp_of(path: &Path) -> Stamp {
    block_on(RealFs.load(path)).expect("load").stamp
}

/// For a save of a path that does not exist yet. Never compared: `save` stats
/// nothing when there is no file to stat, so any value does.
fn absent() -> Stamp {
    Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH),
        size: 0,
    }
}

/// Every `.md` file in the directory, by name.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .expect("read_dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
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

    block_on(RealFs.save(&note, "hello\nworld\n", absent())).expect("save");

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

    let saved = block_on(RealFs.save(&note, "after", stamp_of(&note))).expect("save");

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

    block_on(RealFs.save(&note, "after", stamp_of(&note))).expect("save");

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

    block_on(RealFs.save(&note, "text", absent())).expect("save");

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

    let saved = block_on(RealFs.save(&note, "after", stamp_of(&note))).expect("save");

    assert_eq!(saved.identity.inode, before, "the inode must not move");
    assert_eq!(
        fs::read_to_string(&other).expect("read"),
        "after",
        "the other name sees the write, as it did before"
    );
}

/// A note reached through a symlink — which only a file named on the command
/// line can be — is written through the link. Renaming over it would leave the
/// user with a regular file where their link was, and the real note untouched.
#[test]
fn save_writes_through_a_symlink_rather_than_replacing_it() {
    let dir = TempDir::new("symlink");
    let target = dir.0.join("real.md");
    let link = dir.0.join("link.md");
    fs::write(&target, "before").expect("write");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    block_on(RealFs.save(&link, "after", stamp_of(&link))).expect("save");

    assert!(
        fs::symlink_metadata(&link).expect("stat").is_symlink(),
        "the link must survive its own save"
    );
    assert_eq!(fs::read_to_string(&target).expect("read"), "after");
}

/// An external write between the load and the save. Its bytes survive under a
/// new name, and the note itself holds ours.
#[test]
fn save_with_a_stale_stamp_preserves_the_bytes_it_would_have_destroyed() {
    let dir = TempDir::new("conflict");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let stamp = stamp_of(&note);

    // The other writer. The differing length is what this test relies on; the
    // sleep is there so the mtime differs too, on a filesystem coarse enough
    // that two writes in one second share a timestamp.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "someone else's much longer text").expect("write");

    let saved = block_on(RealFs.save(&note, "ours", stamp)).expect("save");

    let preserved = saved.preserved.expect("a conflict file");
    assert_eq!(
        fs::read_to_string(&preserved).expect("read"),
        "someone else's much longer text"
    );
    assert_eq!(fs::read_to_string(&note).expect("read"), "ours");
    assert_eq!(
        preserved.extension().and_then(|ext| ext.to_str()),
        Some("md"),
        "an extension read_dir filters out would hide the only signal there is"
    );
}

/// The self-healing property: the stamp a save returns is the expectation the
/// next save of that note passes with. Without it every second keystroke would
/// mint a conflict file holding the note's own text.
#[test]
fn save_reports_a_stamp_the_next_save_accepts() {
    let dir = TempDir::new("chain");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");

    let first = block_on(RealFs.save(&note, "one", stamp_of(&note))).expect("save");
    let second = block_on(RealFs.save(&note, "two", first.stamp)).expect("save");

    assert!(second.preserved.is_none());
    assert_eq!(fs::read_to_string(&note).expect("read"), "two");
    assert_eq!(names(&dir.0), vec!["note.md"]);
}

/// A note deleted underneath us is recreated rather than resurrected into a
/// conflict file: there are no bytes to preserve.
#[test]
fn save_of_a_missing_note_recreates_it_and_preserves_nothing() {
    let dir = TempDir::new("missing");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let stamp = stamp_of(&note);
    fs::remove_file(&note).expect("remove");

    let saved = block_on(RealFs.save(&note, "ours", stamp)).expect("save");

    assert!(saved.preserved.is_none());
    assert_eq!(fs::read_to_string(&note).expect("read"), "ours");
}

/// The ordering that makes the whole design safe. If the preserve fails, the
/// replace must not happen: the file keeps the other writer's bytes.
#[test]
fn a_failed_preserve_leaves_the_file_untouched() {
    let dir = TempDir::new("preserve-fails");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let stamp = stamp_of(&note);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "theirs").expect("write");
    // A conflict file cannot be created in a directory with no write
    // permission, while the note itself stays writable in place. Only the
    // preserve step can fail, which is what this isolates.
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o500)).expect("chmod");

    let result = block_on(RealFs.save(&note, "ours", stamp));

    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(result.is_err(), "the save must refuse");
    assert_eq!(
        fs::read_to_string(&note).expect("read"),
        "theirs",
        "the bytes the save could not preserve are still there"
    );
}

/// `replace` restores the note's mode onto its temp file, and
/// `save_preserves_the_file_mode` pins that. The conflict file is a second
/// fresh file holding the same note's content and owes the same.
#[test]
fn a_preserved_file_keeps_the_note_mode() {
    let dir = TempDir::new("conflict-mode");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    fs::set_permissions(&note, fs::Permissions::from_mode(0o600)).expect("chmod");
    let stamp = stamp_of(&note);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "theirs and longer").expect("write");

    let saved = block_on(RealFs.save(&note, "ours", stamp)).expect("save");

    let preserved = saved.preserved.expect("a conflict file");
    let mode = fs::metadata(&preserved).expect("stat").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
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

/// The identity is what `Vault::apply` keys every `EntryId` on, so a rename
/// that minted a new inode would silently detach the note from its open buffer.
#[test]
fn rename_moves_the_note_and_keeps_its_identity() {
    let dir = TempDir::new("rename-identity");
    let from = dir.0.join("before.md");
    let to = dir.0.join("after.md");
    fs::write(&from, "text").expect("write");
    let before = fs::metadata(&from).expect("stat").ino();

    block_on(RealFs.rename(&from, &to)).expect("rename");

    assert_eq!(names(&dir.0), vec!["after.md".to_owned()]);
    assert_eq!(fs::read_to_string(&to).expect("read"), "text");
    assert_eq!(
        fs::metadata(&to).expect("stat").ino(),
        before,
        "a rename must not change the inode"
    );
}

/// The one cell of the collision table `rename(2)` does not refuse for us:
/// "if new exists, it is first removed". Nothing else in the suite would
/// notice the `hard_link` guard being replaced by a bare `fs::rename`.
#[test]
fn rename_refuses_to_destroy_a_different_note() {
    let dir = TempDir::new("rename-collision");
    let from = dir.0.join("a.md");
    let to = dir.0.join("b.md");
    fs::write(&from, "mine").expect("write");
    fs::write(&to, "theirs").expect("write");

    let result = block_on(RealFs.rename(&from, &to));

    assert!(result.is_err(), "the destination is a different note");
    assert_eq!(fs::read_to_string(&to).expect("read"), "theirs");
    assert_eq!(fs::read_to_string(&from).expect("read"), "mine");
}

/// APFS is case-insensitive-but-preserving by default, so `hard_link` reports
/// `EEXIST` here even though both names are one file. Without the
/// same-identity branch a user cannot fix the capitalisation of their own note.
///
/// Cannot run against `MemFs`, which keys a case-sensitive `BTreeMap` and so
/// reports success whatever the branch does.
#[test]
fn rename_can_change_only_the_case_of_a_name() {
    let dir = TempDir::new("rename-case");
    let from = dir.0.join("notes.md");
    let to = dir.0.join("Notes.md");
    fs::write(&from, "text").expect("write");
    let before = fs::metadata(&from).expect("stat").ino();

    block_on(RealFs.rename(&from, &to)).expect("a case-only rename is a rename");

    assert_eq!(names(&dir.0), vec!["Notes.md".to_owned()]);
    assert_eq!(fs::metadata(&to).expect("stat").ino(), before);
}

/// The same trap through a second door: macOS compares names
/// normalization-insensitively, so the NFC and NFD spellings of `Réunion` are
/// one file. A French vault reaches this on its first accented note.
#[test]
fn rename_can_change_only_the_unicode_normalization_of_a_name() {
    let dir = TempDir::new("rename-normalization");
    // Composed: U+00E9. Decomposed: U+0065 U+0301. Written out rather than
    // normalized at runtime, so the test states which bytes it means.
    let from = dir.0.join("R\u{e9}union.md");
    let to = dir.0.join("Re\u{301}union.md");
    fs::write(&from, "text").expect("write");
    let before = fs::metadata(&from).expect("stat").ino();

    block_on(RealFs.rename(&from, &to)).expect("a normalization-only rename is a rename");

    assert_eq!(fs::metadata(&to).expect("stat").ino(), before);
    assert_eq!(names(&dir.0).len(), 1, "one file, not two spellings of it");
}

/// POSIX forbids a hard link to a directory, so a folder takes the fallback
/// branch. It must still move, and still refuse a destination it would destroy.
#[test]
fn rename_moves_a_folder_with_its_subtree() {
    let dir = TempDir::new("rename-folder");
    let from = dir.0.join("projects");
    let to = dir.0.join("archive");
    fs::create_dir(&from).expect("create dir");
    fs::write(from.join("plan.md"), "p").expect("write");
    let before = fs::metadata(&from).expect("stat").ino();

    block_on(RealFs.rename(&from, &to)).expect("rename the folder");

    assert_eq!(names(&dir.0), vec!["archive".to_owned()]);
    assert_eq!(fs::read_to_string(to.join("plan.md")).expect("read"), "p");
    assert_eq!(fs::metadata(&to).expect("stat").ino(), before);
}

/// `read_dir` keeps symlinks out of a vault, but a file named on the command
/// line never passed that filter. `hard_link` resolves the link and would
/// leave a plain file where the link was, stranding its target.
#[test]
fn rename_of_a_symlink_moves_the_link_not_its_target() {
    let dir = TempDir::new("rename-symlink");
    let target = dir.0.join("target.md");
    let link = dir.0.join("link.md");
    let moved = dir.0.join("moved.md");
    fs::write(&target, "text").expect("write");
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");

    block_on(RealFs.rename(&link, &moved)).expect("rename the link");

    assert!(
        fs::symlink_metadata(&moved).expect("stat").is_symlink(),
        "the link must survive as a link"
    );
    assert!(target.exists(), "its target must be untouched");
}

#[test]
fn create_makes_an_empty_note_and_refuses_an_occupied_name() {
    let dir = TempDir::new("create");
    let path = dir.0.join("new.md");

    let identity = block_on(RealFs.create(&path, Kind::Note)).expect("create");

    assert_eq!(fs::read_to_string(&path).expect("read"), "");
    assert_eq!(identity.inode, fs::metadata(&path).expect("stat").ino());
    assert!(
        block_on(RealFs.create(&path, Kind::Note)).is_err(),
        "a create must never take over an existing note"
    );
}

#[test]
fn create_makes_a_folder_the_scan_lists() {
    let dir = TempDir::new("create-folder");
    let path = dir.0.join("projects");

    block_on(RealFs.create(&path, Kind::Dir)).expect("create");

    let entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, Kind::Dir);
}
