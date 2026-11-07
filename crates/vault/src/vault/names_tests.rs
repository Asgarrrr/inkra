//! `Vault::name_taken` (create-rename-delete spec).

use super::test_helpers::{id_of, scan};
use super::*;
use crate::mem_fs::MemFs;

fn id(vault: &Vault, path: &str) -> EntryId {
    id_of(vault, path).expect("an entry")
}

#[test]
fn a_name_matching_a_root_note_is_taken() {
    let fs = MemFs::new();
    fs.insert_file("/vault/plan.md", "");
    let vault = scan(&fs);

    assert!(vault.name_taken(None, Kind::Note, "plan", None));
}

#[test]
fn a_note_collides_whatever_the_case_of_its_extension() {
    let fs = MemFs::new();
    fs.insert_file("/vault/sub/plan.MD", "");
    let vault = scan(&fs);
    let sub = id(&vault, "sub");

    for typed in ["plan", "plan.md", "plan.MD"] {
        assert!(
            vault.name_taken(Some(sub), Kind::Note, typed, None),
            "{typed}"
        );
    }
}

#[test]
fn the_entry_being_renamed_does_not_collide_with_itself() {
    let fs = MemFs::new();
    fs.insert_file("/vault/plan.MD", "");
    fs.insert_file("/vault/sub/plan.MD", "");
    let vault = scan(&fs);
    let sub = id(&vault, "sub");

    for (parent, path) in [(None, "plan.MD"), (Some(sub), "sub/plan.MD")] {
        let own = id(&vault, path);
        assert!(!vault.name_taken(parent, Kind::Note, "plan", Some(own)));
        assert!(vault.name_taken(parent, Kind::Note, "plan", None));
    }
}

#[test]
fn a_note_in_another_folder_does_not_collide() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/sub");
    fs.insert_file("/vault/other/plan.md", "");
    let vault = scan(&fs);
    let sub = id(&vault, "sub");

    assert!(!vault.name_taken(Some(sub), Kind::Note, "plan", None));
}

#[test]
fn a_note_below_the_parents_children_does_not_collide() {
    let fs = MemFs::new();
    fs.insert_file("/vault/sub/x/plan.md", "");
    let vault = scan(&fs);
    let (sub, x) = (id(&vault, "sub"), id(&vault, "sub/x"));

    assert!(!vault.name_taken(None, Kind::Note, "plan", None));
    assert!(!vault.name_taken(Some(sub), Kind::Note, "plan", None));
    assert!(vault.name_taken(Some(x), Kind::Note, "plan", None));
}

#[test]
fn a_folder_collides_with_a_folder_of_the_same_name() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/d");
    let vault = scan(&fs);

    assert!(vault.name_taken(None, Kind::Dir, " d ", None));
}

#[test]
fn a_folder_name_on_disk_is_compared_untrimmed() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/ d");
    let vault = scan(&fs);
    id(&vault, " d");

    assert!(!vault.name_taken(None, Kind::Dir, "d", None));
}

#[test]
fn an_asset_does_not_collide_with_a_note_of_its_stem() {
    let fs = MemFs::new();
    fs.insert_bytes("/vault/photo.png", b"\x89PNG".to_vec());
    let vault = scan(&fs);

    assert!(!vault.name_taken(None, Kind::Note, "photo", None));
    assert!(vault.name_taken(None, Kind::Asset, "photo.png", None));
}

#[test]
fn a_parent_that_is_gone_answers_for_the_root() {
    let fs = MemFs::new();
    fs.insert_file("/vault/plan.md", "");
    let vault = scan(&fs);

    assert!(vault.name_taken(Some(EntryId::DETACHED), Kind::Note, "plan", None));
}
