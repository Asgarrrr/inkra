use std::path::{Path, PathBuf};

use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Vault};

use super::Moves;

/// A vault listing `a.md`, and its id.
fn vault() -> (Vault, EntryId) {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "alpha");
    let vault =
        futures::executor::block_on(Vault::scan(&fs, Path::new("/vault"))).expect("scan the vault");
    let a = vault
        .entries()
        .find(|entry| entry.display_name() == "a")
        .map(|entry| entry.id)
        .expect("a is listed");
    (vault, a)
}

#[test]
fn a_failed_rename_restores_the_earlier_move() {
    let (vault, a) = vault();
    let mut moves = Moves::default();
    moves.moved(a, PathBuf::from("/vault/z.md"));

    let begun = moves.begin(a);
    moves.fail(a, begun);

    assert_eq!(moves.resolve(a, &vault), Some(PathBuf::from("/vault/z.md")));
}

#[test]
fn settling_an_older_move_keeps_the_newer_one() {
    let (vault, a) = vault();
    let mut moves = Moves::default();
    moves.moved(a, PathBuf::from("/vault/z.md"));
    moves.moved(a, PathBuf::from("/vault/y.md"));

    moves.settle(a, Path::new("/vault/z.md"));

    assert_eq!(moves.resolve(a, &vault), Some(PathBuf::from("/vault/y.md")));
}

#[test]
fn a_rename_in_flight_resolves_to_no_path() {
    let (vault, a) = vault();
    let mut moves = Moves::default();

    let _begun = moves.begin(a);

    assert_eq!(moves.resolve(a, &vault), None);
}
