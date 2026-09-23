use std::path::Path;

use futures::executor::block_on;

use super::*;
use crate::mem_fs::MemFs;

fn scan(fs: &MemFs) -> Vault {
    block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
}

fn ordered_paths(fs: &MemFs) -> Vec<String> {
    scan(fs)
        .entries()
        .map(|e| e.path.as_path().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_scan_finds_notes_in_subdirectories() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/sub/b.md", "b");

    let vault = scan(&fs);

    let names: Vec<_> = vault.entries().map(|e| e.path.display_name()).collect();
    assert!(names.contains(&"a".to_owned()));
    assert!(names.contains(&"b".to_owned()));
}

#[test]
fn an_empty_directory_is_kept_as_an_entry() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/empty");

    let vault = scan(&fs);

    assert_eq!(vault.entries().count(), 1);
    assert_eq!(vault.entries().next().map(|e| e.kind), Some(Kind::Dir));
}

#[test]
fn an_unreadable_subdirectory_does_not_lose_the_vault() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_dir("/vault/locked");
    fs.make_unreadable("/vault/locked");

    let vault = scan(&fs);

    assert!(vault.entries().any(|e| e.path.display_name() == "a"));
}

#[test]
fn a_rename_keeps_the_entry_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut vault = scan(&fs);
    let id = vault.entries().next().expect("one entry").id;

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    block_on(vault.rescan(&fs)).expect("rescan");

    let entry = vault.entry(id).expect("the id must survive a rename");
    assert_eq!(entry.path.display_name(), "b");
}

#[test]
fn a_file_recreated_at_the_same_path_gets_a_new_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "first");
    let mut vault = scan(&fs);
    let old = vault.entries().next().expect("one entry").id;

    fs.remove(Path::new("/vault/a.md"));
    fs.insert_file("/vault/a.md", "second");
    block_on(vault.rescan(&fs)).expect("rescan");

    let new = vault.entries().next().expect("one entry").id;
    assert_ne!(old, new, "a different file must not inherit the id");
}

#[test]
fn a_hard_link_does_not_shift_the_surviving_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.hard_link(Path::new("/vault/a.md"), "/vault/b.md");
    let mut vault = scan(&fs);
    let before: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
    assert_eq!(before.len(), 1, "one file, one entry");

    block_on(vault.rescan(&fs)).expect("rescan");

    let after: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
    assert_eq!(
        before, after,
        "nothing changed on disk, so no id may change"
    );
}

#[test]
fn a_save_keeps_the_entry_id_across_a_rescan() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let id = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    vault.reidentify(id, saved.identity);
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(
        vault.entry(id).map(|e| e.path.display_name()).as_deref(),
        Some("a")
    );
}

#[test]
fn a_save_without_reidentify_loses_the_entry_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let id = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    block_on(vault.rescan(&fs)).expect("rescan");

    assert!(vault.entry(id).is_none(), "the old id names nothing");
}

#[test]
fn reidentify_ignores_an_entry_a_rescan_already_replaced() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let stale = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    block_on(vault.rescan(&fs)).expect("rescan");
    let minted = vault.entries().next().expect("one entry").id;
    vault.reidentify(stale, saved.identity);
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(
        vault.entries().next().map(|e| e.id),
        Some(minted),
        "the id the rescan committed is the one that survives"
    );
}

#[test]
fn the_scan_never_mints_the_detached_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/sub/b.md", "b");

    assert!(scan(&fs).entries().all(|e| e.id != EntryId::DETACHED));
}

#[test]
fn entry_resolves_every_id_after_a_rescan_reorders_the_list() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a/c.md", "c");
    fs.insert_file("/vault/b.md", "b");

    let vault = scan(&fs);

    for entry in vault.entries() {
        let found = vault
            .entry(entry.id)
            .expect("every id the scan minted must resolve");
        assert_eq!(found.id, entry.id, "entry() returned the wrong row");
        assert_eq!(
            found.path, entry.path,
            "entry() returned a row for a different path"
        );
    }
}

#[test]
fn a_folder_note_does_not_split_its_folder_from_its_children() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects.md", "n");

    assert_eq!(
        ordered_paths(&fs),
        vec!["Projects", "Projects/plan.md", "Projects.md"]
    );
}

#[test]
fn a_directorys_children_follow_it_rather_than_all_other_directories() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a/z.md", "z");
    fs.insert_file("/vault/c/y.md", "y");
    fs.insert_file("/vault/b.md", "b");

    assert_eq!(
        ordered_paths(&fs),
        vec!["a", "a/z.md", "c", "c/y.md", "b.md"]
    );
}

/// The invariant the sidebar's collapse-skip rests on.
#[test]
fn every_directorys_descendants_are_contiguous() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects/deep/x.md", "x");
    fs.insert_file("/vault/Projects.md", "n");
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/zz/other.md", "o");

    let vault = scan(&fs);
    let entries: Vec<_> = vault.entries().collect();

    for (i, entry) in entries.iter().enumerate() {
        if entry.kind != Kind::Dir {
            continue;
        }
        let descendants = entries
            .iter()
            .filter(|c| c.path != entry.path && c.path.starts_with(&entry.path))
            .count();
        for offset in 1..=descendants {
            assert!(
                entries[i + offset].path.starts_with(&entry.path),
                "{:?} is not contiguous: index {} breaks the run",
                entry.path.as_path(),
                i + offset
            );
        }
    }
}
