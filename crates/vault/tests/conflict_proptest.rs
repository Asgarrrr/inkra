//! No save destroys bytes it did not write.
//!
//! The property needed one correction the first run forced. "Whatever was at
//! the path before a save is still readable after it" is false, and rightly so:
//! a save whose expectation matches the file is replacing bytes *it* wrote, and
//! is allowed to. Two external writes in a row lose the first for the same
//! reason — that is the external writer's doing, not ours.
//!
//! What must hold is conditional on the expectation. When it matches the file,
//! nothing is preserved. When it does not, the file belongs to someone else and
//! every one of its bytes must survive under a new name.

// `[lints] workspace = true` applies to every target of the crate, integration
// tests included, so `expect_used` is denied here too. Same reasoning as
// `tests/real_fs.rs`: a broken setup call must name itself.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Stamp};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    /// A second process rewriting the note in place. `insert_file` keeps the
    /// path's identity, which is what makes it that and not a save.
    External(String),
    /// Our write, carrying the expectation our last save produced.
    Save(String),
    /// Our write carrying a deliberately stale expectation: the one from before
    /// the previous save. Models a write issued before an earlier one landed.
    SaveStale(String),
}

fn text() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "a".to_owned(),
        "bb".to_owned(),
        "ccc".to_owned(),
        String::new(),
    ])
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        text().prop_map(Op::External),
        text().prop_map(Op::Save),
        text().prop_map(Op::SaveStale),
    ]
}

/// Every file under `/vault`, by content. A conflict file is an ordinary note,
/// so `read_dir` lists it like any other.
fn contents(fs: &MemFs) -> Vec<String> {
    block_on(fs.read_dir(Path::new("/vault")))
        .expect("read_dir")
        .into_iter()
        .filter_map(|entry| fs.read_to_string(&entry.path))
        .collect()
}

proptest! {
    #[test]
    fn a_save_never_destroys_what_it_found(ops in prop::collection::vec(op(), 1..20)) {
        let fs = MemFs::new();
        let note = PathBuf::from("/vault/Note.md");
        fs.insert_file(note.clone(), "initial");

        let mut current: Stamp = block_on(fs.load(&note)).expect("load").stamp;
        let mut previous: Stamp = current;

        for op in ops {
            let (text, expected) = match op {
                Op::External(text) => {
                    fs.insert_file(note.clone(), text);
                    continue;
                }
                Op::Save(text) => (text, current),
                Op::SaveStale(text) => (text, previous),
            };

            let found = fs.read_to_string(&note).expect("the note exists");
            let actual = block_on(fs.load(&note)).expect("load").stamp;
            let saved = block_on(fs.save(&note, &text, expected)).expect("save");

            if actual == expected {
                prop_assert!(
                    saved.preserved.is_none(),
                    "a save that found what it expected replaces its own bytes"
                );
            } else {
                let preserved = saved
                    .preserved
                    .clone()
                    .ok_or_else(|| TestCaseError::fail("a stale expectation must preserve"))?;
                let kept = fs.read_to_string(&preserved);
                prop_assert_eq!(
                    kept.as_deref(),
                    Some(found.as_str()),
                    "the other writer's bytes must survive verbatim"
                );
            }
            prop_assert!(contents(&fs).contains(&text), "our own write landed");

            previous = current;
            current = saved.stamp;
        }
    }
}
