#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::path::Path;

use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Vault};
use proptest::prelude::*;

use super::rows::{Item, Rows};

fn scanned(fs: &MemFs) -> Vault {
    futures::executor::block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
}

fn rescanned(vault: &mut Vault, fs: &MemFs) {
    futures::executor::block_on(vault.rescan(fs)).expect("rescan");
}

fn id(vault: &Vault, name: &str) -> EntryId {
    vault
        .entries()
        .find(|entry| entry.path.as_path() == Path::new(name))
        .map(|entry| entry.id)
        .expect("the entry is in the vault")
}

fn flat(names: &[&str]) -> MemFs {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    for name in names {
        fs.insert_file(format!("/vault/{name}.md"), "text");
    }
    fs
}

fn walked(vault: &Vault) -> Rows<()> {
    let mut rows = Rows::default();
    rows.rewalk(vault.shared_entries(), &HashSet::new());
    rows
}

fn names(rows: &Rows<()>) -> Vec<Option<String>> {
    (0..rows.item_count())
        .map(|item| match rows.item(item).expect("inside the list") {
            Item::Row(row) => Some(rows.visible().row(row).name),
            Item::Field(()) => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum Slot {
    None,
    Insert(usize),
    Replace(usize),
}

/// A flat vault of `0..=12` notes and any slot that fits it.
fn vault_and_slot() -> impl Strategy<Value = (usize, Slot)> {
    (0..=12_usize).prop_flat_map(|n| {
        let insert = (0..=n).prop_map(Slot::Insert).boxed();
        let slot = if n == 0 {
            prop_oneof![Just(Slot::None), insert].boxed()
        } else {
            prop_oneof![Just(Slot::None), insert, (0..n).prop_map(Slot::Replace)].boxed()
        };
        (Just(n), slot)
    })
}

proptest! {
    #[test]
    fn every_item_maps_to_one_row_or_the_field((n, slot) in vault_and_slot()) {
        let names: Vec<String> = (0..n).map(|ix| format!("note-{ix:02}")).collect();
        let vault = scanned(&flat(&names.iter().map(String::as_str).collect::<Vec<_>>()));
        let mut rows = walked(&vault);
        let replaced = match slot {
            Slot::None => None,
            Slot::Insert(item) => {
                rows.insert(item, ());
                None
            }
            Slot::Replace(row) => {
                let target = rows.visible().entry(row).id;
                prop_assert_eq!(rows.replace(target, ()), Some(row));
                prop_assert_eq!(rows.target(), Some(target));
                Some(row)
            }
        };

        let inserting = matches!(slot, Slot::Insert(_));
        prop_assert_eq!(rows.item_count(), n + usize::from(inserting));
        let items: Vec<Item<()>> = (0..rows.item_count()).map(|item| rows.item(item).unwrap()).collect();
        prop_assert_eq!(rows.item(rows.item_count()), None);

        let listed: Vec<usize> = items
            .iter()
            .filter_map(|item| match item {
                Item::Row(row) => Some(*row),
                Item::Field(()) => None,
            })
            .collect();
        let expected: Vec<usize> = (0..n).filter(|&row| Some(row) != replaced).collect();
        prop_assert_eq!(listed, expected);

        let fields: Vec<usize> = (0..items.len()).filter(|&item| items[item] == Item::Field(&())).collect();
        let field = match slot {
            Slot::None => None,
            Slot::Insert(item) | Slot::Replace(item) => Some(item),
        };
        prop_assert_eq!(fields, field.into_iter().collect::<Vec<_>>());
        prop_assert_eq!(rows.field_item(), field);
        prop_assert_eq!(rows.edit().is_some(), field.is_some());
    }
}

#[test]
fn a_create_after_a_shrink_is_clamped_to_the_new_row_count() {
    let fs = flat(&["a", "b", "c"]);
    let mut vault = scanned(&fs);
    let mut rows = walked(&vault);
    rows.insert(3, ());

    fs.remove(Path::new("/vault/c.md"));
    fs.remove(Path::new("/vault/b.md"));
    rescanned(&mut vault, &fs);

    assert_eq!(
        rows.rewalk(vault.shared_entries(), &HashSet::new()),
        Some(1)
    );
    assert_eq!(names(&rows), [Some("a".to_owned()), None]);
}

#[test]
fn a_create_that_still_fits_keeps_its_item_after_a_walk() {
    let fs = flat(&["a", "b", "c"]);
    let mut vault = scanned(&fs);
    let mut rows = walked(&vault);
    rows.insert(1, ());

    fs.insert_file("/vault/d.md", "text");
    rescanned(&mut vault, &fs);

    assert_eq!(
        rows.rewalk(vault.shared_entries(), &HashSet::new()),
        Some(1)
    );
    let name = |name: &str| Some(name.to_owned());
    assert_eq!(
        names(&rows),
        [name("a"), None, name("b"), name("c"), name("d")]
    );
}

#[test]
fn a_rename_follows_its_target_when_a_note_sorts_above_it() {
    let fs = flat(&["a", "b"]);
    let mut vault = scanned(&fs);
    let b = id(&vault, "b.md");
    let mut rows = walked(&vault);
    assert_eq!(rows.replace(b, ()), Some(1));

    fs.insert_file("/vault/0.md", "text");
    rescanned(&mut vault, &fs);

    assert_eq!(
        rows.rewalk(vault.shared_entries(), &HashSet::new()),
        Some(2)
    );
    assert_eq!(rows.target(), Some(b));
    assert_eq!(
        names(&rows),
        [Some("0".to_owned()), Some("a".to_owned()), None]
    );
}

#[test]
fn a_rename_closes_when_its_target_moves_under_a_collapsed_folder() {
    let fs = flat(&["a"]);
    fs.insert_file("/vault/Archive/old.md", "o");
    let mut vault = scanned(&fs);
    let a = id(&vault, "a.md");
    let mut rows = walked(&vault);
    rows.replace(a, ());

    futures::executor::block_on(inkra_vault::Fs::rename(
        &fs,
        Path::new("/vault/a.md"),
        Path::new("/vault/Archive/a.md"),
    ))
    .expect("rename");
    rescanned(&mut vault, &fs);
    assert!(vault.entry(a).is_some(), "the move keeps the id");

    assert_eq!(rows.rewalk(vault.shared_entries(), &HashSet::new()), None);
    assert!(rows.edit().is_none());
    assert_eq!(rows.item_count(), 1);
}

#[test]
fn a_rename_closes_when_its_target_is_gone() {
    let fs = flat(&["a", "b"]);
    let mut vault = scanned(&fs);
    let b = id(&vault, "b.md");
    let mut rows = walked(&vault);
    rows.replace(b, ());

    fs.remove(Path::new("/vault/b.md"));
    rescanned(&mut vault, &fs);

    assert_eq!(rows.rewalk(vault.shared_entries(), &HashSet::new()), None);
    assert!(rows.edit().is_none());
    assert_eq!(rows.target(), None);
}

#[test]
fn replacing_a_hidden_id_opens_nothing() {
    let fs = MemFs::new();
    fs.insert_file("/vault/Archive/old.md", "o");
    let vault = scanned(&fs);
    let old = id(&vault, "Archive/old.md");
    let mut rows = walked(&vault);

    assert_eq!(rows.replace(old, ()), None);
    assert!(rows.edit().is_none());
    assert_eq!(rows.item_count(), 1);
}

#[test]
fn closing_hands_back_the_edit_and_restores_the_rows() {
    let vault = scanned(&flat(&["a", "b"]));
    let mut rows: Rows<&str> = Rows::default();
    rows.rewalk(vault.shared_entries(), &HashSet::new());
    rows.insert(1, "field");

    assert_eq!(rows.close(), Some("field"));
    assert_eq!(rows.close(), None);
    assert_eq!(rows.item_count(), 2);
    assert_eq!(rows.item(1), Some(Item::Row(1)));
}
