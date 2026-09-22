//! Entry ids survive an arbitrary sequence of vault mutations.
//!
//! Path-keyed ids fail this: a rename is a delete plus an insert, so the id
//! changes exactly when it must not. See the design spec's "Identity" section.

// `expect_used` is denied here too (`[lints] workspace = true`) — see
// `tests/real_fs.rs`.
#![allow(clippy::expect_used)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Add(String),
    Remove(usize),
    Rename(usize, String),
    /// `Notes.md` → `notes.md`. `MemFs` is case-sensitive, so this is a plain
    /// rename of one inode — which is what a case-insensitive, case-preserving
    /// filesystem reports for a recase, and `Vault` has no case-folding branch
    /// for it to take. It reproduces the shape the spec's "Identity" section
    /// names, not a distinct code path.
    RecaseName(usize),
    /// Writing a note replaces it: `RealFs::save` renames a temp file over the
    /// destination, so the inode moves and `Vault::reidentify` has to carry the
    /// id across. The only mutation here that changes a file's identity without
    /// changing its path.
    Save(usize),
}

fn name() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "Alpha".to_owned(),
        "Beta".to_owned(),
        "Gamma".to_owned(),
        "Delta".to_owned(),
    ])
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        name().prop_map(Op::Add),
        (0usize..8).prop_map(Op::Remove),
        (0usize..8, name()).prop_map(|(i, n)| Op::Rename(i, n)),
        (0usize..8).prop_map(Op::RecaseName),
        (0usize..8).prop_map(Op::Save),
    ]
}

proptest! {
    #[test]
    fn identity_survives_arbitrary_mutation(ops in prop::collection::vec(op(), 1..25)) {
        let fs = MemFs::new();
        fs.insert_file("/vault/Alpha.md", "seed");

        let mut vault = block_on(Vault::scan(&fs, Path::new("/vault"))).expect("scan");

        let mut expected: Vec<(EntryId, PathBuf)> = vault
            .entries()
            .map(|e| (e.id, e.path.as_path().to_path_buf()))
            .collect();
        // Ids whose file is gone. A removed file's id must never be handed to
        // a different file: `(device, inode)` identity mints a fresh id per
        // inode, and anything that compacts or recycles ids breaks the open
        // buffer holding the old one.
        let mut retired: HashSet<EntryId> = HashSet::new();

        for operation in ops {
            // The path `Op::Add` created this round, if any. Its id is not
            // known until the rescan below mints one.
            let mut added: Option<PathBuf> = None;

            match operation {
                Op::Add(n) => {
                    let absolute = PathBuf::from(format!("/vault/{n}.md"));
                    if fs.read_to_string(&absolute).is_none() {
                        fs.insert_file(absolute, "new");
                        added = Some(PathBuf::from(format!("{n}.md")));
                    }
                }
                Op::Remove(i) => {
                    if let Some((_, path)) = expected.get(i) {
                        fs.remove(&PathBuf::from("/vault").join(path));
                    }
                }
                Op::Rename(i, n) => {
                    if let Some((id, path)) = expected.get(i).cloned() {
                        let from = PathBuf::from("/vault").join(&path);
                        let to = format!("/vault/{n}.md");
                        if fs.read_to_string(Path::new(&to)).is_none() {
                            block_on(fs.rename(&from, Path::new(&to))).expect("rename");
                            expected[i] = (id, PathBuf::from(format!("{n}.md")));
                        }
                    }
                }
                Op::RecaseName(i) => {
                    if let Some((id, path)) = expected.get(i).cloned() {
                        let from = PathBuf::from("/vault").join(&path);
                        let lowered = path.to_string_lossy().to_lowercase();
                        let to = PathBuf::from("/vault").join(&lowered);
                        // Same occupied-destination guard as `Op::Rename`.
                        // `Fs::rename` refuses one rather than overwriting, so
                        // without this the harness would `expect` on a failure
                        // the operation is entitled to report.
                        if fs.read_to_string(&from).is_some() && fs.read_to_string(&to).is_none() {
                            block_on(fs.rename(&from, &to)).expect("rename");
                            expected[i] = (id, PathBuf::from(lowered));
                        }
                    }
                }
                Op::Save(i) => {
                    if let Some((id, path)) = expected.get(i).cloned() {
                        let absolute = PathBuf::from("/vault").join(&path);
                        if fs.read_to_string(&absolute).is_some() {
                            let stamp = block_on(fs.load(&absolute)).expect("load").stamp;
                            let saved =
                                block_on(fs.save(&absolute, "written", stamp)).expect("save");
                            // The path is unchanged, so `expected` keeps its
                            // entry; only the identity behind it moved.
                            vault.reidentify(id, saved.identity);
                        }
                    }
                }
            }

            block_on(vault.rescan(&fs)).expect("rescan");

            // A file added this round joins the tracked set with whatever id
            // the rescan minted, so every later round checks it too. Without
            // this the property covers only the seed entry, and the "adds" in
            // the test's own name are never verified.
            if let Some(path) = added {
                let id = vault
                    .entries()
                    .find(|entry| entry.path.as_path() == path)
                    .map(|entry| entry.id);
                prop_assert!(id.is_some(), "a file added at {path:?} produced no entry");
                if let Some(id) = id {
                    expected.push((id, path));
                }
            }

            for (id, path) in &expected {
                let absolute = PathBuf::from("/vault").join(path);
                if fs.read_to_string(&absolute).is_some() {
                    let entry = vault.entry(*id);
                    prop_assert!(
                        entry.is_some(),
                        "id {id:?} vanished while {path:?} is still on disk"
                    );
                    if let Some(entry) = entry {
                        prop_assert_eq!(entry.path.as_path(), path.as_path());
                    }
                }
            }
            for entry in vault.entries() {
                prop_assert!(
                    !retired.contains(&entry.id),
                    "id {:?} was retired with its file and reused for {:?}",
                    entry.id,
                    entry.path.as_path()
                );
            }

            // Must agree across whatever reordering this round's mutation caused.
            for entry in vault.entries() {
                let looked_up = vault.entry(entry.id);
                prop_assert!(
                    looked_up.is_some(),
                    "entry({:?}) returned None for a listed id",
                    entry.id
                );
                if let Some(looked_up) = looked_up {
                    prop_assert_eq!(looked_up.id, entry.id);
                    prop_assert_eq!(looked_up.path.as_path(), entry.path.as_path());
                }
            }
            // An id no scan has ever minted must never resolve.
            prop_assert!(vault.entry(EntryId::DETACHED).is_none());

            expected.retain(|(id, p)| {
                let live = fs.read_to_string(&PathBuf::from("/vault").join(p)).is_some();
                if !live {
                    retired.insert(*id);
                }
                live
            });
        }
    }
}
