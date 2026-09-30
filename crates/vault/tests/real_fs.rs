//! `RealFs` against a real temporary directory. Separate from the unit tests
//! because it touches the disk.

#![allow(clippy::expect_used)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use futures::StreamExt;
use futures::executor::block_on;
use inkra_vault::{Fs, Kind, Mtime, RealFs, Stamp, VaultError, Watch};

/// A temp directory that removes itself.
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

fn stamp_of(path: &Path) -> Stamp {
    block_on(RealFs.load(path)).expect("load").stamp
}

/// For a save of a path that does not exist yet. Never compared: no file
/// means nothing to stat.
fn absent() -> Stamp {
    Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH),
        size: 0,
    }
}

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

#[test]
fn save_with_a_stale_stamp_preserves_the_bytes_it_would_have_destroyed() {
    let dir = TempDir::new("conflict");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let stamp = stamp_of(&note);

    // Sleep so the mtime differs too: some filesystems share a timestamp
    // across writes within one second.
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

/// A present note whose folder left must not recreate it; a gone note's write creates it first (watcher spec, F9).
#[test]
fn save_to_a_path_whose_folders_are_missing_fails_until_create_parents() {
    let dir = TempDir::new("missing-folders");
    let note = dir.0.join("f").join("g").join("note.md");

    let Err(VaultError::Io { source, .. }) = block_on(RealFs.save(&note, "ours", absent())) else {
        panic!("a missing folder must fail");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    assert!(names(&dir.0).is_empty(), "{:?}", names(&dir.0));

    block_on(RealFs.create_parents(&note)).expect("create parents");
    block_on(RealFs.create_parents(&note)).expect("again, over existing folders");
    let saved = block_on(RealFs.save(&note, "ours", absent())).expect("save");

    assert!(saved.preserved.is_none());
    assert_eq!(fs::read_to_string(&note).expect("read"), "ours");
    assert_eq!(names(&dir.0.join("f").join("g")), ["note.md"]);
}

#[test]
fn a_failed_preserve_leaves_the_file_untouched() {
    let dir = TempDir::new("preserve-fails");
    let note = dir.0.join("note.md");
    fs::write(&note, "before").expect("write");
    let stamp = stamp_of(&note);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "theirs").expect("write");
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

#[test]
fn read_dir_accepts_md_in_any_case() {
    let dir = TempDir::new("md-case");
    for file in ["a.md", "B.MD", "c.Md", "d.mdx", "e.markdown"] {
        fs::write(dir.0.join(file), "x").expect("write");
    }

    let entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");

    let mut paths: Vec<_> = entries.iter().map(|entry| entry.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![dir.0.join("B.MD"), dir.0.join("a.md"), dir.0.join("c.Md")]
    );
    assert!(entries.iter().all(|entry| entry.kind == Kind::Note));
}

#[test]
fn read_dir_skips_other_extensions_and_symlinks() {
    let dir = TempDir::new("filters");
    fs::write(dir.0.join("a.md"), "a").expect("write");
    fs::write(dir.0.join("notes.txt"), "t").expect("write");
    std::os::unix::fs::symlink(std::env::temp_dir(), dir.0.join("escape.md"))
        .expect("create symlink");

    let entries = block_on(RealFs.read_dir(&dir.0)).expect("read_dir");

    let paths: Vec<_> = entries.iter().map(|entry| entry.path.clone()).collect();
    assert_eq!(paths, vec![dir.0.join("a.md")]);
}

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

/// APFS is case-insensitive-but-preserving by default, so this only exercises
/// the same-identity branch on real disk, never on the case-sensitive `MemFs`.
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

/// macOS compares names normalization-insensitively, so NFC and NFD spellings
/// of the same name are one file.
#[test]
fn rename_can_change_only_the_unicode_normalization_of_a_name() {
    let dir = TempDir::new("rename-normalization");
    // Composed (U+00E9) vs decomposed (U+0065 U+0301).
    let from = dir.0.join("R\u{e9}union.md");
    let to = dir.0.join("Re\u{301}union.md");
    fs::write(&from, "text").expect("write");
    let before = fs::metadata(&from).expect("stat").ino();

    block_on(RealFs.rename(&from, &to)).expect("a normalization-only rename is a rename");

    assert_eq!(fs::metadata(&to).expect("stat").ino(), before);
    assert_eq!(names(&dir.0).len(), 1, "one file, not two spellings of it");
}

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

#[test]
fn check_dir_accepts_a_folder_and_refuses_the_rest() {
    let dir = TempDir::new("check-dir");
    fs::write(dir.0.join("a.md"), "").expect("write");

    assert!(block_on(RealFs.check_dir(&dir.0)).is_ok());
    assert!(
        block_on(RealFs.check_dir(&dir.0.join("a.md"))).is_err(),
        "a file"
    );
    assert!(block_on(RealFs.check_dir(&dir.0.join("missing"))).is_err());
}

/// The kind `walk_changed` reads as gone, and `MemFs` matches.
#[test]
fn read_dir_of_a_file_is_not_a_directory() {
    let dir = TempDir::new("read-dir-file");
    fs::write(dir.0.join("a.md"), "").expect("write");

    let Err(VaultError::Io { source, .. }) = block_on(RealFs.read_dir(&dir.0.join("a.md"))) else {
        panic!("a file is not a folder");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotADirectory);
}

/// `FSEvents` reports resolved paths: the event strips against the canonical root, not the link.
#[test]
fn watch_through_a_symlink_reports_a_new_note_under_one_of_its_roots() {
    let dir = TempDir::new("watch");
    let real = dir.0.join("real");
    fs::create_dir(&real).expect("create the watched folder");
    let link = dir.0.join("link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let Watch {
        mut events,
        root,
        resolved,
        watcher,
    } = RealFs.watch(&link).expect("watch");
    // A plain thread, so the wait is a bounded `recv_timeout`, not a sleep.
    let (forward, received) = mpsc::channel();
    std::thread::spawn(move || {
        while let Some(event) = block_on(events.next()) {
            if forward.send(event).is_err() {
                break;
            }
        }
    });

    fs::write(link.join("a.md"), "a").expect("write");

    let deadline = Instant::now() + Duration::from_secs(1);
    let seen = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(event) = received.recv_timeout(left) else {
            panic!("no event for a.md within a second");
        };
        if [&root, &resolved]
            .iter()
            .any(|root| event.path.strip_prefix(root) == Ok(Path::new("a.md")))
        {
            break event;
        }
    };
    assert!(!seen.rescan);
    drop(watcher);
}

#[test]
fn replace_creates_missing_folders_overwrites_and_leaves_no_temp_file() {
    let dir = TempDir::new("replace-app-state");
    let file = dir.0.join("sessions").join("a.json");

    block_on(RealFs.replace(&file, "one")).expect("replace under missing folders");
    block_on(RealFs.replace(&file, "two")).expect("replace over an existing file");

    assert_eq!(fs::read_to_string(&file).expect("read"), "two");
    assert_eq!(names(&dir.0.join("sessions")), ["a.json"]);
}
