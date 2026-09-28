#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::path::Path;

use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Vault};

use super::visible::Visible;

fn scanned(fs: &MemFs) -> Vault {
    futures::executor::block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
}

fn id(vault: &Vault, name: &str) -> EntryId {
    vault
        .entries()
        .find(|entry| entry.path.as_path() == Path::new(name))
        .map(|entry| entry.id)
        .expect("the entry is in the vault")
}

fn walked(vault: &Vault, open: &[&str], target: Option<EntryId>) -> (Vec<String>, Option<usize>) {
    let expanded: HashSet<EntryId> = open.iter().map(|name| id(vault, name)).collect();
    let mut visible = Visible::default();
    let at = visible.rewalk(vault.shared_entries(), &expanded, target);
    let labels = (0..visible.len())
        .map(|ix| {
            let row = visible.row(ix);
            match row.kind {
                Kind::Dir => format!("{}/", row.name),
                Kind::Note => row.name,
            }
        })
        .collect();
    (labels, at)
}

fn tree() -> Vault {
    let fs = MemFs::new();
    fs.insert_dir("/vault/Empty");
    fs.insert_file("/vault/Projects/deep/x.md", "x");
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects 2/y.md", "y");
    fs.insert_dir("/vault/Projects 2/Last");
    fs.insert_file("/vault/Projects.md", "n");
    scanned(&fs)
}

#[test]
fn a_collapsed_folder_skips_its_subtree_and_no_further() {
    let (labels, _) = walked(&tree(), &[], None);
    assert_eq!(
        labels,
        ["Empty/", "Projects/", "Projects 2/", "Projects"],
        "an empty folder, and a name extending a folder's, stay outside its subtree"
    );
}

#[test]
fn an_expanded_folder_with_a_collapsed_one_inside_resumes_after_it() {
    let (labels, _) = walked(&tree(), &["Projects"], None);
    assert_eq!(
        labels,
        [
            "Empty/",
            "Projects/",
            "deep/",
            "plan",
            "Projects 2/",
            "Projects"
        ]
    );
}

#[test]
fn a_levels_notes_stop_at_their_folders_end() {
    let (labels, _) = walked(&tree(), &["Projects", "Projects/deep", "Projects 2"], None);
    assert_eq!(
        labels,
        [
            "Empty/",
            "Projects/",
            "deep/",
            "x",
            "plan",
            "Projects 2/",
            "Last/",
            "y",
            "Projects"
        ],
    );
}

#[test]
fn a_collapsed_folder_as_the_last_entry_ends_the_walk() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/Z/z.md", "z");
    let vault = scanned(&fs);
    let z = id(&vault, "Z");
    let mut visible = Visible::default();
    visible.rewalk(vault.shared_entries(), &HashSet::new(), None);
    assert_eq!(visible.len(), 2);
    assert_eq!(
        visible.entry(0).id,
        z,
        "folders first, so `Z/` precedes `a`"
    );
}

#[test]
fn a_target_lands_on_its_row_not_its_entry_position() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Archive/old.md", "o");
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/b.md", "b");
    let vault = scanned(&fs);
    let b = id(&vault, "b.md");
    assert_eq!(walked(&vault, &[], Some(b)).1, Some(2));
    assert_eq!(walked(&vault, &["Archive"], Some(b)).1, Some(3));
}

#[test]
fn a_target_under_a_collapsed_folder_has_no_row() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Archive/old.md", "o");
    let vault = scanned(&fs);
    let old = id(&vault, "Archive/old.md");
    assert_eq!(walked(&vault, &[], Some(old)).1, None);
    assert_eq!(walked(&vault, &["Archive"], Some(old)).1, Some(1));
}

#[test]
fn a_parent_is_the_folder_holding_the_row() {
    let fs = MemFs::new();
    fs.insert_file("/vault/A/B/n.md", "n");
    fs.insert_file("/vault/A/m.md", "m");
    let vault = scanned(&fs);
    let expanded: HashSet<EntryId> = [id(&vault, "A"), id(&vault, "A/B")].into();
    let mut visible = Visible::default();
    visible.rewalk(vault.shared_entries(), &expanded, None);
    let parents: Vec<_> = (0..visible.len()).map(|ix| visible.parent_of(ix)).collect();
    let (a, b) = (Some(id(&vault, "A")), Some(id(&vault, "A/B")));
    assert_eq!(parents, [None, a, b, a], "A/, B/, n, m");
}
