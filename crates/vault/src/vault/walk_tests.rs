use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::{paths, rel, scan};
use super::*;
use crate::mem_fs::MemFs;

const ROOT: &str = "/vault";

fn walk(fs: &MemFs, from: &str) -> Walk {
    block_on(Vault::walk(fs, Path::new(ROOT), &rel(from), &Serial)).expect("walk")
}

fn id_of(vault: &Vault, path: &str) -> Option<EntryId> {
    vault
        .entries()
        .find(|e| e.path.as_path() == Path::new(path))
        .map(|e| e.id)
}

#[test]
fn a_walk_from_a_folder_reads_only_below_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    fs.insert_file("/vault/f/g/c.md", "");

    let walk = walk(&fs, "f");

    let mut read: Vec<_> = walk
        .read
        .iter()
        .map(|r| r.as_path().to_path_buf())
        .collect();
    read.sort();
    assert_eq!(read, [Path::new("f"), Path::new("f/g")]);
    assert_eq!(walk.found.len(), 3, "b.md, g and c.md");
}

#[test]
fn a_walk_fails_when_its_start_cannot_be_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    fs.make_unreadable("/vault/f");

    assert!(block_on(Vault::walk(&fs, Path::new(ROOT), &rel("f"), &Serial)).is_err());
}

#[test]
fn a_walk_from_a_file_fails() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");

    assert!(block_on(Vault::walk(&fs, Path::new(ROOT), &rel("a.md"), &Serial)).is_err());
}

#[test]
fn a_walk_from_a_missing_path_fails() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");

    assert!(block_on(Vault::walk(&fs, Path::new(ROOT), &rel("gone"), &Serial)).is_err());
}

#[test]
fn a_folder_that_fails_mid_walk_keeps_its_entries() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    let mut vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    fs.make_unreadable("/vault/f");
    fs.insert_file("/vault/c.md", "");
    block_on(vault.rescan(&fs)).expect("the root reads");

    assert_eq!(id_of(&vault, "f/b.md"), Some(b), "kept, under the same id");
    assert!(
        id_of(&vault, "c.md").is_some(),
        "the rest of the scan still lands"
    );
}

#[test]
fn a_folder_renamed_mid_walk_keeps_its_notes_until_the_next_scan() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    let mut vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    // The root lists `f`; `f` is renamed before its own listing.
    let gate = fs.block_read_dir("/vault/f");
    let whole = RelPath::root();
    let walked = block_on(async {
        let walk = Vault::walk(&fs, Path::new(ROOT), &whole, &Serial);
        let rename = async {
            fs.rename(Path::new("/vault/f"), Path::new("/vault/g"))
                .await
                .expect("rename");
            gate.release();
        };
        futures::join!(walk, rename).0
    })
    .expect("the root reads");
    vault = vault.applied(walked);
    assert_eq!(id_of(&vault, "f/b.md"), Some(b));

    block_on(vault.rescan(&fs)).expect("rescan");
    assert_eq!(id_of(&vault, "g/b.md"), Some(b));
    assert_eq!(id_of(&vault, "f/b.md"), None);
}

#[test]
fn a_file_the_walk_found_elsewhere_leaves_the_folder_it_did_not_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    fs.insert_dir("/vault/g");
    let vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    block_on(fs.rename(Path::new("/vault/f/b.md"), Path::new("/vault/g/b.md"))).expect("move");
    let vault = vault.applied(walk(&fs, "g"));

    assert_eq!(id_of(&vault, "g/b.md"), Some(b));
    assert_eq!(id_of(&vault, "f/b.md"), None, "one file, one entry");
}

#[test]
fn an_entry_goes_with_a_folder_the_walk_no_longer_found() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/b.md", "");
    let vault = scan(&fs);

    fs.remove(Path::new("/vault/f"));
    let root_only = Walk {
        read: vec![RelPath::root()],
        found: block_on(fs.read_dir(Path::new(ROOT))).expect("root"),
    };
    let vault = vault.applied(root_only);

    assert!(paths(&vault).is_empty(), "{:?}", paths(&vault));
}

#[test]
fn a_rescan_drops_a_note_its_folder_no_longer_lists() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    fs.insert_file("/vault/f/c.md", "");
    let mut vault = scan(&fs);

    fs.remove(Path::new("/vault/f/c.md"));
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(paths(&vault), ["f", "f/b.md", "a.md"], "c.md left `f`");
}

#[test]
fn a_root_pass_lists_the_root_level_and_keeps_nothing_below() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    let root_only = Walk {
        read: vec![RelPath::root()],
        found: block_on(fs.read_dir(Path::new(ROOT))).expect("root"),
    };

    let vault = Vault::new(Path::new(ROOT)).applied(root_only);

    assert_eq!(paths(&vault), ["f", "a.md"]);
    assert!(!vault.is_scanned());
}

/// Across folders: the walk's stack reads `b` before `a`, so walk order would mint `b/y.md` first.
#[test]
fn fresh_ids_follow_path_order() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a/z.md", "");
    fs.insert_file("/vault/b/y.md", "");

    let vault = scan(&fs);

    let ids: Vec<_> = vault.entries().map(|e| e.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "a, a/z.md, b, b/y.md, minted in that order");
}

#[test]
fn a_hard_link_keeps_the_path_that_sorts_first() {
    let fs = MemFs::new();
    fs.insert_file("/vault/z/x.md", "");
    fs.insert_dir("/vault/a");
    fs.hard_link(Path::new("/vault/z/x.md"), "/vault/a/y.md");

    let notes: Vec<_> = scan(&fs)
        .entries()
        .filter(|e| e.kind == Kind::Note)
        .map(|e| e.path.as_path().to_string_lossy().into_owned())
        .collect();

    assert_eq!(notes, ["a/y.md"]);
}

#[test]
fn a_vault_is_scanned_once_a_full_scan_applied() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    let mut vault = Vault::new(Path::new(ROOT));
    assert!(!vault.is_scanned());
    assert!(
        !vault.applied(walk(&fs, "")).is_scanned(),
        "applied copies the flag"
    );

    block_on(vault.rescan(&fs)).expect("rescan");
    assert!(vault.is_scanned());

    let mut unread = Vault::new(Path::new(ROOT));
    unread.mark_scanned();
    assert!(unread.is_scanned());
}

#[test]
fn a_clone_shares_the_entries() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    let vault = scan(&fs);

    let copy = vault.clone();

    assert!(Arc::ptr_eq(&vault.shared_entries(), &copy.shared_entries()));
}

#[test]
fn a_walk_reads_folders_down_to_the_depth_limit() {
    let fs = MemFs::new();
    let deep: Vec<&str> = vec!["d"; MAX_DEPTH + 2];
    fs.insert_file(format!("/vault/{}/x.md", deep.join("/")), "");

    let deepest = walk(&fs, "")
        .found
        .iter()
        .map(|child| {
            child
                .path
                .strip_prefix(ROOT)
                .expect("under the root")
                .components()
                .count()
        })
        .max();

    assert_eq!(
        deepest,
        Some(MAX_DEPTH + 1),
        "the folder at the limit is read, the next is not"
    );
}

/// A partial walk keeps the id of a note that left the folders it read (O3).
#[test]
fn a_rename_out_of_a_partial_walk_keeps_its_id_for_the_next_scan() {
    let fs = MemFs::new();
    fs.insert_file("/vault/under/a.md", "");
    fs.insert_dir("/vault/outside");
    let mut vault = scan(&fs);
    let original = id_of(&vault, "under/a.md").expect("a");

    block_on(fs.rename(
        Path::new("/vault/under/a.md"),
        Path::new("/vault/outside/a.md"),
    ))
    .expect("rename");
    vault = vault.applied(walk(&fs, "under"));
    assert!(
        id_of(&vault, "outside/a.md").is_none(),
        "not seen: outside of the walk"
    );
    assert!(
        id_of(&vault, "under/a.md").is_none(),
        "gone: `under` was read and no longer holds it"
    );

    block_on(vault.rescan(&fs)).expect("rescan");
    assert_eq!(
        id_of(&vault, "outside/a.md"),
        Some(original),
        "the inode did not change across the rename: the same id was expected, not a fresh one"
    );
}

/// The inode a save replaced may survive a partial walk; the id goes to the inode the save made.
#[test]
fn a_reclaim_takes_its_id_from_an_identity_a_partial_walk_kept() {
    let fs = MemFs::new();
    fs.insert_file("/vault/under/a.md", "");
    let mut vault = scan(&fs);
    let ours = id_of(&vault, "under/a.md").expect("a");
    let path = Path::new("/vault/under/a.md");
    let loaded = block_on(fs.load(path)).expect("load");
    let saved = block_on(fs.save(path, "written", loaded.stamp)).expect("save");

    vault = vault.applied(walk(&fs, "under"));
    assert!(vault.reidentify(ours, saved.identity, saved.stamp.mtime));

    assert_eq!(id_of(&vault, "under/a.md"), Some(ours));
    assert_eq!(
        vault.id_for(loaded.identity),
        None,
        "one id, one identity: the replaced inode no longer answers for our note"
    );
}

/// A save's inode that a partial walk kept on a fresh id with no entry maps back to our note.
#[test]
fn reidentify_remaps_an_identity_a_partial_walk_kept_on_no_entry() {
    let fs = MemFs::new();
    fs.insert_file("/vault/under/a.md", "");
    fs.insert_dir("/vault/outside");
    let mut vault = scan(&fs);
    let ours = id_of(&vault, "under/a.md").expect("a");
    let path = Path::new("/vault/under/a.md");
    let loaded = block_on(fs.load(path)).expect("load");
    let saved = block_on(fs.save(path, "written", loaded.stamp)).expect("save");
    vault = vault.applied(walk(&fs, "under"));
    let minted = id_of(&vault, "under/a.md").expect("the saved inode, under a fresh id");
    block_on(fs.rename(path, Path::new("/vault/outside/a.md"))).expect("rename");
    vault = vault.applied(walk(&fs, "under"));
    assert_eq!(vault.id_for(saved.identity), Some(minted));

    assert!(!vault.reidentify(ours, saved.identity, saved.stamp.mtime));
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(id_of(&vault, "outside/a.md"), Some(ours));
}

/// A walk from the root answers for every identity, even when a folder fails (O3).
#[test]
fn a_root_walk_that_failed_a_folder_drops_the_identity_that_moved_into_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_dir("/vault/f");
    let mut vault = scan(&fs);
    let a = id_of(&vault, "a.md").expect("a");

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/f/a.md"))).expect("move");
    fs.make_unreadable("/vault/f");
    block_on(vault.rescan(&fs)).expect("the root reads");
    // `make_unreadable` holds one path: `f` reads again.
    fs.make_unreadable("/nowhere");
    block_on(vault.rescan(&fs)).expect("rescan");

    let moved = id_of(&vault, "f/a.md").expect("found once `f` reads");
    assert_ne!(moved, a, "the trade-off: unseen, it gets a fresh id");
}

/// A root pass reads the root: it drops the identities it did not see, as any root walk.
#[test]
fn a_root_pass_drops_the_identity_that_moved_into_a_folder() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_dir("/vault/f");
    let mut vault = scan(&fs);
    let a = id_of(&vault, "a.md").expect("a");

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/f/a.md"))).expect("move");
    vault = vault.applied(Walk {
        read: vec![RelPath::root()],
        found: block_on(fs.read_dir(Path::new(ROOT))).expect("root"),
    });
    block_on(vault.rescan(&fs)).expect("rescan");

    let moved = id_of(&vault, "f/a.md").expect("found by the full scan");
    assert_ne!(moved, a);
}

/// What a root pass did not read stays a live entry, so its identity stays with it.
#[test]
fn a_root_pass_keeps_the_entries_and_identities_under_a_folder_it_did_not_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    let vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");
    let identity = block_on(fs.load(Path::new("/vault/f/b.md")))
        .expect("load")
        .identity;

    let mut vault = vault.applied(Walk {
        read: vec![RelPath::root()],
        found: block_on(fs.read_dir(Path::new(ROOT))).expect("root"),
    });
    assert_eq!(id_of(&vault, "f/b.md"), Some(b));
    assert_eq!(vault.id_for(identity), Some(b));

    block_on(vault.rescan(&fs)).expect("rescan");
    assert_eq!(id_of(&vault, "f/b.md"), Some(b));
}

/// A root walk with a folder that fails still drops a deleted note's inode (O3).
#[test]
fn a_reused_inode_gets_a_fresh_id_despite_a_folder_that_fails() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/locked/b.md", "");
    let mut vault = scan(&fs);
    let a = id_of(&vault, "a.md").expect("a");
    let identity = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .identity;
    fs.make_unreadable("/vault/locked");

    fs.remove(Path::new("/vault/a.md"));
    block_on(vault.rescan(&fs)).expect("the root reads");
    fs.insert_file_with_identity("/vault/c.md", "unrelated", identity);
    block_on(vault.rescan(&fs)).expect("the root reads");

    let c = id_of(&vault, "c.md").expect("c");
    assert_ne!(c, a, "an unrelated file took a deleted note's id");
    assert!(
        id_of(&vault, "locked/b.md").is_some(),
        "the folder that fails keeps its entry"
    );
}
