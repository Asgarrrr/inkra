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
                Kind::Note | Kind::Asset => row.name,
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

fn flat(names: &[&str]) -> (Vault, Visible) {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    for name in names {
        fs.insert_file(format!("/vault/{name}.md"), "text");
    }
    let vault = scanned(&fs);
    let mut visible = Visible::default();
    visible.rewalk(vault.shared_entries(), &HashSet::new(), None);
    (vault, visible)
}

#[test]
fn a_leaving_row_hands_its_place_to_the_row_below_else_above() {
    let (vault, visible) = flat(&["a", "b", "c"]);
    let [a, b, c] = ["a.md", "b.md", "c.md"].map(|name| id(&vault, name));
    assert_eq!(visible.neighbour(b), Some(c));
    assert_eq!(visible.neighbour(a), Some(b));
    assert_eq!(visible.neighbour(c), Some(b));

    let (lone_vault, lone) = flat(&["a"]);
    assert_eq!(lone.neighbour(id(&lone_vault, "a.md")), None);
    assert_eq!(visible.neighbour(EntryId::DETACHED), None, "an unknown id");
}

#[test]
fn a_step_enters_at_an_end_and_clamps_at_both() {
    let (vault, visible) = flat(&["a", "b", "c"]);
    let [a, b, c] = ["a.md", "b.md", "c.md"].map(|name| id(&vault, name));
    assert_eq!(
        visible.step(None, 1),
        Some(a),
        "down enters at the first row"
    );
    assert_eq!(visible.step(None, -1), Some(c), "up enters at the last row");
    assert_eq!(visible.step(Some(a), 1), Some(b));
    assert_eq!(visible.step(Some(b), -1), Some(a));
    assert_eq!(visible.step(Some(a), -1), Some(a));
    assert_eq!(visible.step(Some(c), 1), Some(c));
    assert_eq!(
        visible.step(Some(EntryId::DETACHED), 1),
        Some(a),
        "no row enters too"
    );

    let (_, empty) = flat(&[]);
    assert_eq!(empty.step(None, 1), None);
    assert_eq!(empty.step(Some(a), -1), None);
}

#[test]
fn an_expanded_folder_draws_its_notes_and_assets_in_name_order() {
    let fs = MemFs::new();
    for file in [
        "Media/a.md",
        "Media/b.png",
        "Media/c.md",
        "Media/d.pdf",
        "Z/z.png",
    ] {
        fs.insert_file(format!("/vault/{file}"), "");
    }
    fs.insert_file("/vault/top.md", "");
    let (labels, _) = walked(&scanned(&fs), &["Media"], None);
    assert_eq!(labels, ["Media/", "a", "b.png", "c", "d.pdf", "Z/", "top"]);
}
