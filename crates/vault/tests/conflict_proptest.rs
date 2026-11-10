//! No save destroys bytes it did not write. Conditional on the expectation:
//! a save that finds what it expected is replacing its own bytes, and may.

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Stamp};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    /// A second process rewriting the note in place.
    External(String),
    /// Our write, carrying the expectation our last save produced.
    Save(String),
    /// Our write, carrying the expectation from before the previous save —
    /// models a write issued before an earlier one landed.
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
