//! `order::position`: the row at a path, of a kind.

use super::Vault;
use super::order::position;
use super::test_helpers::{id_of, rel, scan};
use crate::fs::Kind;
use crate::mem_fs::MemFs;

fn tree() -> Vault {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/f/b.md", "b");
    fs.insert_file("/vault/f/photo.png", "p");
    fs.insert_dir("/vault/f/g");
    fs.insert_dir("/vault/x.md");
    fs.insert_file("/vault/z.md", "z");
    scan(&fs)
}

#[test]
fn position_finds_each_row_at_its_path_and_kind() {
    let vault = tree();
    let entries = &vault.state.entries;

    for (path, kind) in [
        ("a.md", Kind::Note),
        ("f", Kind::Dir),
        ("f/b.md", Kind::Note),
        ("f/photo.png", Kind::Asset),
        ("f/g", Kind::Dir),
        ("x.md", Kind::Dir),
        ("z.md", Kind::Note),
    ] {
        let at = position(entries, &rel(path), kind);
        assert_eq!(at.map(|at| entries[at].id), id_of(&vault, path), "{path}");
        assert!(at.is_some(), "{path}");
    }
}

#[test]
fn position_finds_nothing_at_a_missing_path() {
    let vault = tree();

    for path in ["b.md", "f/a.md", "f/g/c.md", "y"] {
        for kind in [Kind::Note, Kind::Dir, Kind::Asset] {
            assert_eq!(
                position(&vault.state.entries, &rel(path), kind),
                None,
                "{path}"
            );
        }
    }
}

#[test]
fn position_refuses_a_row_of_another_kind() {
    let vault = tree();

    for (path, kind) in [
        ("a.md", Kind::Dir),
        ("a.md", Kind::Asset),
        ("f/photo.png", Kind::Note),
        ("f/g", Kind::Note),
        ("x.md", Kind::Note),
    ] {
        assert_eq!(
            position(&vault.state.entries, &rel(path), kind),
            None,
            "{path}"
        );
    }
}
