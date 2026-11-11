use futures::FutureExt;
use futures::executor::block_on;

use super::MemFs;
use crate::fs::{Fs, Kind, MAX_TEXT_BYTES, Mtime, New, Stamp, VaultError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn stamp_of(fs: &MemFs, path: &str) -> Stamp {
    let Ok(loaded) = block_on(fs.load(Path::new(path))) else {
        panic!("load must succeed");
    };
    loaded.stamp
}

#[test]
fn inserting_a_file_creates_its_parent_directories() {
    let fs = MemFs::new();
    fs.insert_file("/vault/sub/b.md", "b");

    let Ok(root) = block_on(fs.read_dir(Path::new("/vault"))) else {
        panic!("read_dir must succeed");
    };

    assert_eq!(root.len(), 1, "only the direct child is listed");
    assert_eq!(root[0].path, Path::new("/vault/sub"));
    assert_eq!(root[0].kind, Kind::Dir);
}

#[test]
fn loads_are_counted_per_path_found_or_not() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let _ = block_on(fs.load(Path::new("/vault/a.md")));
    let _ = block_on(fs.load(Path::new("/vault/a.md")));
    let _ = block_on(fs.load(Path::new("/vault/gone.md")));

    assert_eq!(fs.loads(Path::new("/vault/a.md")), 2);
    assert_eq!(fs.loads(Path::new("/vault/gone.md")), 1);
    assert_eq!(fs.loads(Path::new("/vault/other.md")), 0);
}

#[test]
fn read_dir_lists_notes_in_any_case_and_other_files_as_assets() {
    let fs = MemFs::new();
    for file in ["a.md", "B.MD", "c.Md", "d.txt", "e.markdown", "md"] {
        fs.insert_file(format!("/vault/{file}"), "x");
    }

    let Ok(entries) = block_on(fs.read_dir(Path::new("/vault"))) else {
        panic!("read_dir must succeed");
    };

    let mut names: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = entry.path.file_name().unwrap_or_default();
            (name.to_string_lossy().into_owned(), entry.kind)
        })
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        names,
        [
            ("B.MD".to_owned(), Kind::Note),
            ("a.md".to_owned(), Kind::Note),
            ("c.Md".to_owned(), Kind::Note),
            ("d.txt".to_owned(), Kind::Asset),
            ("e.markdown".to_owned(), Kind::Asset),
            ("md".to_owned(), Kind::Asset),
        ]
    );
}

#[test]
fn read_dir_lists_direct_children_and_separates_kinds() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_dir("/vault/sub");
    fs.insert_file("/vault/sub/b.md", "b");

    let Ok(entries) = block_on(fs.read_dir(Path::new("/vault"))) else {
        panic!("read_dir must succeed");
    };

    let of_kind = |wanted: Kind| -> Vec<PathBuf> {
        entries
            .iter()
            .filter(|entry| entry.kind == wanted)
            .map(|entry| entry.path.clone())
            .collect()
    };
    assert_eq!(of_kind(Kind::Dir), vec![PathBuf::from("/vault/sub")]);
    assert_eq!(
        of_kind(Kind::Note),
        vec![PathBuf::from("/vault/a.md")],
        "b.md is not a direct child of the root"
    );
}

#[test]
fn an_unreadable_directory_reports_an_error_rather_than_an_empty_listing() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/locked");
    fs.make_unreadable("/vault/locked");

    let result = block_on(fs.read_dir(Path::new("/vault/locked")));

    assert!(matches!(result, Err(VaultError::Io { .. })));
}

#[test]
fn a_rename_keeps_the_identity_and_a_recreated_file_gets_a_new_one() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let Ok(before) = block_on(fs.load(Path::new("/vault/a.md"))) else {
        panic!("load must succeed");
    };

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    let Ok(renamed) = block_on(fs.load(Path::new("/vault/b.md"))) else {
        panic!("load must succeed after the rename");
    };
    assert_eq!(renamed.identity, before.identity);

    fs.remove(Path::new("/vault/b.md"));
    fs.insert_file("/vault/b.md", "second");
    let Ok(recreated) = block_on(fs.load(Path::new("/vault/b.md"))) else {
        panic!("load must succeed after the recreation");
    };
    assert_ne!(recreated.identity, before.identity);
}

#[test]
fn an_unwritable_file_reports_an_error_and_keeps_its_old_text() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    fs.make_unwritable("/vault/a.md");

    let stamp = stamp_of(&fs, "/vault/a.md");
    let result = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp));

    assert!(matches!(result, Err(VaultError::Io { .. })));
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("before"),
        "a failed write leaves the file as it was"
    );
}

#[test]
fn a_blocked_save_does_not_reach_the_file_until_the_gate_releases() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");

    let gate = fs.block_save("/vault/a.md");
    let stamp = stamp_of(&fs, "/vault/a.md");
    let mut write = Box::pin(fs.save(Path::new("/vault/a.md"), "after", stamp));
    assert!(write.as_mut().now_or_never().is_none());
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("before"),
        "the write is in flight and must not have landed"
    );

    gate.release();
    assert!(block_on(write).is_ok());
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("after")
    );
}

#[test]
fn two_writes_of_one_path_get_different_stamps() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let first = stamp_of(&fs, "/vault/a.md");
    fs.insert_file("/vault/a.md", "two");
    let second = stamp_of(&fs, "/vault/a.md");

    assert_ne!(first, second);
}

#[test]
fn a_stale_stamp_preserves_the_external_text() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let stamp = stamp_of(&fs, "/vault/a.md");

    // Keeps the path's identity, as a second process writing in place would.
    fs.insert_file("/vault/a.md", "theirs");

    let Ok(saved) = block_on(fs.save(Path::new("/vault/a.md"), "ours", stamp)) else {
        panic!("save must succeed");
    };

    let Some(preserved) = saved.preserved else {
        panic!("a stale stamp must preserve");
    };
    assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("ours")
    );
}

#[test]
fn a_file_older_than_the_expectation_still_conflicts() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "theirs");
    let actual = stamp_of(&fs, "/vault/a.md");

    let from_the_future = Stamp {
        mtime: Mtime(actual.mtime.0 + Duration::from_mins(1)),
        size: actual.size,
    };
    let Ok(saved) = block_on(fs.save(Path::new("/vault/a.md"), "ours", from_the_future)) else {
        panic!("save must succeed");
    };

    let Some(preserved) = saved.preserved else {
        panic!("a file older than the expectation is still not the file we expected");
    };
    assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
}

#[test]
fn a_conflict_copy_steps_past_a_folder_at_its_name() {
    let fs = MemFs::new();
    let when = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_001_125));
    fs.insert_with_mtime("/vault/Note.md", "theirs", when);
    fs.insert_dir("/vault/Note (conflict 2026-09-21 14-32-05Z).md");
    let stale = Stamp {
        mtime: Mtime(when.0 + Duration::from_mins(1)),
        size: 6,
    };

    let saved = block_on(fs.save(Path::new("/vault/Note.md"), "ours", stale)).ok();

    assert_eq!(
        saved.and_then(|saved| saved.preserved),
        Some(PathBuf::from(
            "/vault/Note (conflict 2026-09-21 14-32-05Z) 2.md"
        ))
    );
    let folder = Path::new("/vault/Note (conflict 2026-09-21 14-32-05Z).md");
    assert!(
        block_on(fs.read_dir(folder)).is_ok(),
        "the folder is still a folder"
    );
}

#[test]
fn the_stamp_a_save_returns_is_accepted_by_the_next_save() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let stamp = stamp_of(&fs, "/vault/a.md");

    let Ok(first) = block_on(fs.save(Path::new("/vault/a.md"), "one", stamp)) else {
        panic!("the first save must succeed");
    };
    let Ok(second) = block_on(fs.save(Path::new("/vault/a.md"), "two", first.stamp)) else {
        panic!("the second save must succeed");
    };

    assert!(second.preserved.is_none());
}

#[test]
fn create_refuses_a_name_that_is_taken() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "alpha");

    assert!(block_on(fs.create(Path::new("/vault/a.md"), New::Note)).is_err());
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
}

#[test]
fn create_mints_an_identity_the_scan_reports() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    let identity = block_on(fs.create(Path::new("/vault/new.md"), New::Note)).expect("create");

    let entries = block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].identity, identity);
}

#[test]
fn rename_carries_the_identity_to_the_new_path() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "alpha");
    let before = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .identity;

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");

    let after = block_on(fs.load(Path::new("/vault/b.md"))).expect("load");
    assert_eq!(after.identity, before);
    assert_eq!(after.text, "alpha");
}

#[test]
fn rename_refuses_an_occupied_destination_and_keeps_both_files() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "mine");
    fs.insert_file("/vault/b.md", "theirs");

    assert!(block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).is_err());

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("mine")
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/b.md")).as_deref(),
        Some("theirs")
    );
}

#[test]
fn renaming_a_folder_moves_its_subtree() {
    let fs = MemFs::new();
    fs.insert_file("/vault/projects/plan.md", "p");
    fs.insert_file("/vault/projects/deep/x.md", "x");

    block_on(fs.rename(Path::new("/vault/projects"), Path::new("/vault/archive"))).expect("rename");

    assert_eq!(
        fs.read_to_string(Path::new("/vault/archive/plan.md"))
            .as_deref(),
        Some("p")
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/archive/deep/x.md"))
            .as_deref(),
        Some("x")
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/projects/plan.md")),
        None
    );
}

#[test]
fn renaming_a_folder_into_itself_is_refused_and_moves_nothing() {
    let fs = MemFs::new();
    fs.insert_file("/vault/projects/plan.md", "p");
    let listed = || block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    let folder = listed()[0].identity;
    let note = block_on(fs.load(Path::new("/vault/projects/plan.md")))
        .expect("load")
        .identity;

    let refused = block_on(fs.rename(
        Path::new("/vault/projects"),
        Path::new("/vault/projects/inner"),
    ));

    assert_eq!(
        refused.err().and_then(|error| error.io_kind()),
        Some(std::io::ErrorKind::InvalidInput)
    );
    let after = listed();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].path, Path::new("/vault/projects"));
    assert_eq!(after[0].identity, folder);
    let kept = block_on(fs.load(Path::new("/vault/projects/plan.md"))).expect("load");
    assert_eq!((kept.text.as_str(), kept.identity), ("p", note));
    assert!(block_on(fs.check_dir(Path::new("/vault/projects/inner"))).is_err());
}

#[test]
fn trash_takes_the_note_out_of_the_vault_and_keeps_its_bytes() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "alpha");

    block_on(fs.trash(Path::new("/vault/a.md"))).expect("trash");

    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
    assert!(
        block_on(fs.read_dir(Path::new("/vault")))
            .expect("read_dir")
            .is_empty()
    );
    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
}

#[test]
fn trash_of_a_missing_note_reports_an_error() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    assert!(block_on(fs.trash(Path::new("/vault/gone.md"))).is_err());
}

#[test]
fn save_round_trips_and_load_normalizes_line_endings() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    // Not compared: no file exists yet to stat.
    let absent = Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH),
        size: 0,
    };
    let Ok(_) = block_on(fs.save(Path::new("/vault/a.md"), "one\r\ntwo", absent)) else {
        panic!("save must succeed");
    };
    let Ok(loaded) = block_on(fs.load(Path::new("/vault/a.md"))) else {
        panic!("load must succeed");
    };

    assert_eq!(loaded.text, "one\ntwo");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("one\r\ntwo"),
        "the stored bytes are untouched; normalization happens on load"
    );
}

/// As `RealFs`: a present note whose folder left must not recreate it; a gone note's write
/// creates it first (watcher spec, F9).
#[test]
fn save_under_a_missing_folder_fails_until_create_parents() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    let absent = Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH),
        size: 0,
    };

    let Err(VaultError::Io { source, .. }) =
        block_on(fs.save(Path::new("/vault/f/a.md"), "ours", absent))
    else {
        panic!("a missing folder must fail");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    assert!(block_on(fs.check_dir(Path::new("/vault/f"))).is_err());
    assert_eq!(fs.read_to_string(Path::new("/vault/f/a.md")), None);

    let Ok(()) = block_on(fs.create_parents(Path::new("/vault/f/a.md"))) else {
        panic!("create_parents must succeed");
    };
    let Ok(_) = block_on(fs.save(Path::new("/vault/f/a.md"), "ours", absent)) else {
        panic!("save must succeed once the folder exists");
    };
    assert_eq!(
        fs.read_to_string(Path::new("/vault/f/a.md")).as_deref(),
        Some("ours")
    );
}

#[test]
fn read_dir_of_a_missing_folder_is_not_found() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    let Err(VaultError::Io { source, .. }) = block_on(fs.read_dir(Path::new("/vault/gone"))) else {
        panic!("a missing folder must fail");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
}

/// As `RealFs`: a file where a folder was is not a missing folder.
#[test]
fn read_dir_of_a_file_is_not_a_directory() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");

    let Err(VaultError::Io { source, .. }) = block_on(fs.read_dir(Path::new("/vault/a.md"))) else {
        panic!("a file is not a folder");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotADirectory);
}

#[test]
fn insert_dir_creates_its_parents() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/a/b");

    let Ok(root) = block_on(fs.read_dir(Path::new("/vault"))) else {
        panic!("read_dir must succeed");
    };

    let listed: Vec<_> = root.iter().map(|entry| entry.path.clone()).collect();
    assert_eq!(listed, [PathBuf::from("/vault/a")]);
    assert_eq!(root[0].kind, Kind::Dir);
}

#[test]
fn a_read_dir_gate_parks_until_released() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");

    let gate = fs.block_read_dir("/vault");
    let (listed, ()) = block_on(async {
        let listing = fs.read_dir(Path::new("/vault"));
        let release = async {
            fs.insert_file("/vault/b.md", "b");
            gate.release();
        };
        futures::join!(listing, release)
    });

    let Ok(listed) = listed else {
        panic!("read_dir must succeed");
    };
    assert_eq!(
        listed.len(),
        2,
        "the listing ran after the gate, and sees b.md"
    );
}

#[test]
fn a_create_gate_parks_until_released() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    let gate = fs.block_create("/vault/b.md");
    let (created, ()) = block_on(async {
        let create = fs.create(Path::new("/vault/b.md"), New::Note);
        let release = async {
            assert_eq!(fs.read_to_string(Path::new("/vault/b.md")), None);
            gate.release();
        };
        futures::join!(create, release)
    });

    assert!(created.is_ok(), "{created:?}");
    assert!(fs.read_to_string(Path::new("/vault/b.md")).is_some());
}

#[test]
fn a_save_parked_after_the_write_already_moved_the_inode() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let path = Path::new("/vault/a.md");
    let Ok(before) = block_on(fs.load(path)) else {
        panic!("load must succeed");
    };

    let gate = fs.block_after_save("/vault/a.md");
    let mut write = Box::pin(fs.save(path, "after", before.stamp));
    assert!(block_on(async { futures::poll!(write.as_mut()) }).is_pending());
    let Ok(during) = block_on(fs.load(path)) else {
        panic!("load must succeed");
    };
    assert_eq!(during.text, "after");
    assert_ne!(during.identity, before.identity);

    gate.release();
    let Ok(saved) = block_on(write) else {
        panic!("the save must succeed");
    };
    assert_eq!(saved.identity, during.identity);
}

#[test]
fn read_dirs_counts_each_listing() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    let _ = block_on(fs.read_dir(Path::new("/vault")));
    let _ = block_on(fs.read_dir(Path::new("/vault")));
    let _ = block_on(fs.read_dir(Path::new("/vault/gone")));

    assert_eq!(fs.read_dirs(Path::new("/vault")), 2);
    assert_eq!(fs.read_dirs(Path::new("/vault/gone")), 1);
    assert_eq!(fs.read_dirs(Path::new("/vault/other")), 0);
}

#[test]
fn check_dir_refuses_a_missing_or_unreadable_folder() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/f");

    assert!(block_on(fs.check_dir(Path::new("/vault/f"))).is_ok());
    assert!(block_on(fs.check_dir(Path::new("/vault/gone"))).is_err());
    fs.make_unreadable("/vault/f");
    assert!(block_on(fs.check_dir(Path::new("/vault/f"))).is_err());
    assert_eq!(fs.read_dirs(Path::new("/vault/f")), 0, "it lists nothing");
}

#[test]
fn replace_creates_missing_folders_and_overwrites() {
    let fs = MemFs::new();
    let path = Path::new("/support/sessions/a.json");

    let Ok(()) = block_on(fs.replace_bytes(path, b"one")) else {
        panic!("replace must succeed under missing folders");
    };
    let Ok(()) = block_on(fs.replace_bytes(path, b"two")) else {
        panic!("replace must succeed over an existing file");
    };

    assert_eq!(fs.read_to_string(path).as_deref(), Some("two"));
    assert!(block_on(fs.check_dir(Path::new("/support/sessions"))).is_ok());
}

#[test]
fn replace_bytes_writes_bytes_that_are_not_utf8_under_missing_folders() {
    let fs = MemFs::new();
    let path = Path::new("/cache/index/a.idx");

    let Ok(()) = block_on(fs.replace_bytes(path, &[0xff, 0x00, 0xfe])) else {
        panic!("replace_bytes must succeed under missing folders");
    };

    let bytes = block_on(fs.read_bytes(path, u64::MAX)).ok();
    assert_eq!(bytes.as_deref(), Some(&[0xff, 0x00, 0xfe][..]));
    let listed = block_on(fs.list_files(Path::new("/cache/index"))).ok();
    assert_eq!(listed, Some(vec![path.to_path_buf()]));
}

#[test]
fn replace_bytes_honours_unwritable() {
    let fs = MemFs::new();
    fs.insert_bytes("/cache/a.idx", b"before".to_vec());
    fs.make_unwritable("/cache/a.idx");

    let result = block_on(fs.replace_bytes(Path::new("/cache/a.idx"), b"after"));

    assert!(matches!(
        result,
        Err(VaultError::Io { source, .. }) if source.kind() == std::io::ErrorKind::PermissionDenied
    ));
    assert_eq!(
        fs.read_to_string(Path::new("/cache/a.idx")).as_deref(),
        Some("before")
    );
}

#[test]
fn list_files_lists_direct_regular_files_sorted() {
    let fs = MemFs::new();
    for file in ["b.json", "a.txt", ".hidden", "sub/nested.json"] {
        fs.insert_file(format!("/vault/themes/{file}"), "x");
    }
    let dir = Path::new("/vault/themes");
    let listed_before = fs.read_dirs(dir);

    let Ok(files) = block_on(fs.list_files(dir)) else {
        panic!("list_files must succeed");
    };

    assert_eq!(
        files,
        [
            PathBuf::from("/vault/themes/a.txt"),
            PathBuf::from("/vault/themes/b.json"),
        ]
    );
    assert_eq!(fs.read_dirs(dir), listed_before, "not a vault listing");
    let missing = block_on(fs.list_files(Path::new("/vault/gone")));
    assert!(matches!(
        missing,
        Err(VaultError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
}

#[test]
fn list_files_skips_save_temp_files_and_non_utf8_dotfiles() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fs = MemFs::new();
    let dir = Path::new("/vault/themes");
    fs.insert_file(dir.join("a.json"), "x");
    fs.insert_file(dir.join("a.json.inkra-1-2.tmp"), "x");
    fs.insert_file(dir.join(OsStr::from_bytes(b".\xff")), "x");

    let files = block_on(fs.list_files(dir)).ok();

    assert_eq!(files, Some(vec![dir.join("a.json")]));
}

#[test]
fn replace_honours_unwritable() {
    let fs = MemFs::new();
    fs.insert_file("/vault/.inkra/settings.json", "before");
    fs.make_unwritable("/vault/.inkra/settings.json");

    let result = block_on(fs.replace_bytes(Path::new("/vault/.inkra/settings.json"), b"after"));

    assert!(matches!(
        result,
        Err(VaultError::Io { source, .. }) if source.kind() == std::io::ErrorKind::PermissionDenied
    ));
    assert_eq!(
        fs.read_to_string(Path::new("/vault/.inkra/settings.json"))
            .as_deref(),
        Some("before")
    );
}

#[test]
fn bytes_that_are_not_utf8_load_as_not_utf8() {
    let fs = MemFs::new();
    fs.insert_bytes("/vault/a.md", [b'a', 0xff]);

    let loaded = block_on(fs.load(Path::new("/vault/a.md")));

    let Err(VaultError::NotUtf8 { path }) = loaded else {
        panic!("not valid UTF-8");
    };
    assert_eq!(path, Path::new("/vault/a.md"));
    assert_eq!(fs.loads(Path::new("/vault/a.md")), 1);
}

#[test]
fn utf8_bytes_load_as_text_stamped_with_their_length() {
    let fs = MemFs::new();
    fs.insert_bytes("/vault/a.md", "été\r\n");

    let Ok(loaded) = block_on(fs.load(Path::new("/vault/a.md"))) else {
        panic!("valid UTF-8");
    };

    assert_eq!(loaded.text, "été\n");
    assert_eq!(
        loaded.stamp.size, 7,
        "the bytes on disk, before the line endings fold"
    );
}

#[test]
#[should_panic(expected = "/vault/a.md")]
fn reading_bytes_that_are_not_utf8_as_a_string_names_the_path() {
    let fs = MemFs::new();
    fs.insert_bytes("/vault/a.md", [0xff]);

    let _ = fs.read_to_string(Path::new("/vault/a.md"));
}

#[test]
fn insert_with_mtime_sets_the_mtime_and_the_size_follows_the_bytes() {
    let fs = MemFs::new();
    let mtime = Mtime(SystemTime::UNIX_EPOCH + Duration::from_millis(1_500));
    fs.insert_with_mtime("/vault/a.md", "héllo", mtime);

    assert_eq!(stamp_of(&fs, "/vault/a.md"), Stamp { mtime, size: 6 });
    let Ok(listed) = block_on(fs.read_dir(Path::new("/vault"))) else {
        panic!("read_dir must succeed");
    };
    assert_eq!((listed[0].mtime, listed[0].size), (mtime, 6));
}

#[test]
fn read_bytes_reads_a_file_of_max_len_and_refuses_one_byte_more() {
    let fs = MemFs::new();
    fs.insert_bytes("/cache/index.idx", b"\x00\xffcache".to_vec());
    let file = Path::new("/cache/index.idx");

    let bytes = block_on(fs.read_bytes(file, 7)).ok();
    let over = block_on(fs.read_bytes(file, 6)).err();

    assert_eq!(bytes.as_deref(), Some(&b"\x00\xffcache"[..]));
    assert_eq!(
        over.and_then(|error| error.io_kind()),
        Some(std::io::ErrorKind::FileTooLarge)
    );
}

#[test]
fn load_refuses_a_file_over_the_cap() {
    let fs = MemFs::new();
    let over = usize::try_from(MAX_TEXT_BYTES + 1).unwrap_or(usize::MAX);
    fs.insert_bytes("/vault/huge.md", vec![b'a'; over]);
    fs.insert_file("/vault/small.md", "four");

    let error = block_on(fs.load(Path::new("/vault/huge.md"))).err();
    let loaded = block_on(fs.load(Path::new("/vault/small.md"))).ok();

    assert_eq!(
        error.and_then(|error| error.io_kind()),
        Some(std::io::ErrorKind::FileTooLarge)
    );
    assert_eq!(loaded.map(|loaded| loaded.text).as_deref(), Some("four"));
}

#[test]
fn load_accepts_a_file_at_the_cap() {
    let fs = MemFs::new();
    let cap = usize::try_from(MAX_TEXT_BYTES).unwrap_or(usize::MAX);
    fs.insert_bytes("/vault/full.md", vec![b'a'; cap]);

    let loaded = block_on(fs.load(Path::new("/vault/full.md"))).ok();

    assert_eq!(loaded.map(|loaded| loaded.text.len()), Some(cap));
}

#[test]
fn read_bytes_of_a_missing_or_unreadable_file_fails() {
    let fs = MemFs::new();
    fs.insert_bytes("/cache/locked.idx", b"x".to_vec());
    fs.make_unreadable("/cache/locked.idx");

    let missing = block_on(fs.read_bytes(Path::new("/cache/gone.idx"), 64)).err();
    let locked = block_on(fs.read_bytes(Path::new("/cache/locked.idx"), 64)).err();

    assert_eq!(
        missing.and_then(|error| error.io_kind()),
        Some(std::io::ErrorKind::NotFound)
    );
    assert_eq!(
        locked.and_then(|error| error.io_kind()),
        Some(std::io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn a_blocked_read_bytes_waits_for_its_gate_and_counts_no_load() {
    let fs = MemFs::new();
    fs.insert_bytes("/cache/index.idx", b"cache".to_vec());
    let file = Path::new("/cache/index.idx");

    let gate = fs.block_read_bytes(file);
    let mut read = Box::pin(fs.read_bytes(file, 64));
    assert!(read.as_mut().now_or_never().is_none());

    gate.release();
    assert_eq!(block_on(read).ok().as_deref(), Some(&b"cache"[..]));
    assert_eq!(fs.loads(file), 0, "`loads` counts the notes' loads only");
    assert_eq!(fs.reads(file), 1);
}

#[test]
fn reads_are_counted_per_path() {
    let fs = MemFs::new();
    let (file, other) = (Path::new("/cache/a.idx"), Path::new("/cache/b.idx"));
    fs.insert_bytes(file, b"a".to_vec());
    fs.insert_bytes(other, b"b".to_vec());

    assert_eq!(fs.reads(file), 0);
    let _ = block_on(fs.read_bytes(file, 64));
    assert_eq!(fs.reads(file), 1);
    let _ = block_on(fs.read_bytes(file, 64));
    assert_eq!(fs.reads(file), 2);
    assert_eq!(fs.reads(other), 0);
}

#[test]
fn remove_file_removes_the_file_and_a_missing_file_is_ok() {
    let fs = MemFs::new();
    let file = Path::new("/cache/index.idx");
    fs.insert_bytes(file, b"cache".to_vec());

    assert!(block_on(fs.remove_file(file)).is_ok());
    assert_eq!(fs.read_to_string(file), None);
    assert!(block_on(fs.remove_file(file)).is_ok(), "already gone");
}

#[test]
fn a_blocked_replace_stores_nothing_until_its_gate_opens() {
    let fs = MemFs::new();
    let file = Path::new("/cache/index.idx");
    fs.insert_bytes(file, b"old".to_vec());

    let gate = fs.block_replace(file);
    let mut replace = Box::pin(fs.replace_bytes(file, b"new"));
    assert!(replace.as_mut().now_or_never().is_none());
    assert_eq!(fs.read_to_string(file).as_deref(), Some("old"));

    gate.release();
    assert!(block_on(replace).is_ok());
    assert_eq!(fs.read_to_string(file).as_deref(), Some("new"));
}
