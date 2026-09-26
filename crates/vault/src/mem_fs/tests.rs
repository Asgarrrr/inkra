use futures::FutureExt;
use futures::executor::block_on;

use super::MemFs;
use crate::fs::{Fs, Kind, Mtime, Stamp, VaultError};
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
fn read_dir_lists_notes_in_any_case_and_nothing_else() {
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
            entry
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["B.MD", "a.md", "c.Md"]);
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

    assert!(block_on(fs.create(Path::new("/vault/a.md"), Kind::Note)).is_err());
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
}

#[test]
fn create_mints_an_identity_the_scan_reports() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    let identity = block_on(fs.create(Path::new("/vault/new.md"), Kind::Note)).expect("create");

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

#[test]
fn read_dir_of_a_missing_folder_is_not_found() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");

    let Err(VaultError::Io { source, .. }) = block_on(fs.read_dir(Path::new("/vault/gone"))) else {
        panic!("a missing folder must fail");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
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
