//! A scan session against the serial history of its saves (open-first spec, O2).

use std::path::Path;

use futures::executor::block_on;
use proptest::prelude::*;

use super::test_helpers::{ROOT, vault, walked};
use super::*;
use crate::Fs;
use crate::mem_fs::MemFs;
use crate::vault::test_helpers::{assert_consistent, id_of, scan};

const NOTES: [&str; 3] = ["a.md", "f/b.md", "c.md"];

/// When a save lands, relative to the one scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    BeforeTheScan,
    BeforeItsWalk,
    AfterItsWalk,
}

fn when() -> impl Strategy<Value = When> {
    prop_oneof![
        Just(When::BeforeTheScan),
        Just(When::BeforeItsWalk),
        Just(When::AfterItsWalk),
    ]
}

/// `in_place`: saved as `RealFs` saves a hard-linked note, keeping the inode; else the inode moves.
fn write(fs: &MemFs, scans: &mut Scans, live: &mut Vault, id: EntryId, note: &str, in_place: bool) {
    let path = Path::new(ROOT).join(note);
    let (identity, mtime) = if in_place {
        fs.insert_file(&path, "written");
        let loaded = block_on(fs.load(&path)).expect("load");
        (loaded.identity, loaded.stamp.mtime)
    } else {
        let stamp = block_on(fs.load(&path)).expect("load").stamp;
        let saved = block_on(fs.save(&path, "saved", stamp)).expect("save");
        (saved.identity, saved.stamp.mtime)
    };
    let _ = scans.reidentify(live, id, identity, mtime);
}

proptest! {
    /// Whenever each save lands, the swap leaves every note under its id, at its path, with the
    /// mtime on disk, as if each save had re-keyed the vault in turn.
    #[test]
    fn a_session_leaves_each_saved_note_under_its_id(
        in_place in any::<[bool; 3]>(),
        saves in prop::collection::vec((0..NOTES.len(), when()), 0..10),
    ) {
        let fs = vault(&NOTES);
        let mut live = scan(&fs);
        let ids: Vec<EntryId> = NOTES.iter().map(|note| id_of(&live, note).expect("scanned")).collect();
        let mut scans = Scans::default();
        let land = |at: When, scans: &mut Scans, live: &mut Vault| {
            for &(note, _) in saves.iter().filter(|(_, when)| *when == at) {
                write(&fs, scans, live, ids[note], NOTES[note], in_place[note]);
            }
        };

        land(When::BeforeTheScan, &mut scans, &mut live);
        let (_, running) = scans.request(&live);
        let running = running.expect("an idle worker starts the scan");
        land(When::BeforeItsWalk, &mut scans, &mut live);
        let scanned = walked(running, &fs);
        land(When::AfterItsWalk, &mut scans, &mut live);
        let (swapped, next) = scans.swap(&mut live, scanned, &Claims::default());

        prop_assert!(swapped.replaced.is_ok());
        prop_assert!(next.is_none());
        for (note, id) in NOTES.iter().zip(&ids) {
            let disk = block_on(fs.load(&Path::new(ROOT).join(note))).expect("load");
            prop_assert_eq!(live.id_for(disk.identity), Some(*id));
            let entry = live.entry(*id).expect("kept");
            prop_assert_eq!(entry.path.as_path(), Path::new(note));
            prop_assert_eq!(entry.mtime, disk.stamp.mtime);
        }
        assert_consistent(&live);
    }
}
