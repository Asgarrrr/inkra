//! The startup timeline's marks.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::TestAppContext;
use inkra_index::live::Status;
use inkra_vault::{Mtime, Stamp};

use super::tests::{indexed_vault, notes_changed, rescan, status};
use crate::test_support::entry_id;
use crate::timeline::Timeline;
use crate::vault::NoteSaved;

#[gpui::test]
fn indexing_marks_names_ready_then_indexed(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let (_fs, _notes, links) =
        indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "[[a]]"), ("c.md", "")]);
    cx.run_until_parked();

    let (bytes, words) = links.read_with(cx, |links, _| {
        let index = links.index();
        (index.corpus().heap_bytes(), index.words().heap_bytes())
    });
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(!line.contains("names ready -"), "{line}");
    // The corpus and words of the end, not of the first build: marked when nothing is left to load.
    assert!(
        line.contains(&format!(
            "(3 notes, corpus {bytes} bytes, words {words} bytes"
        )),
        "{line}"
    );
    assert!(!line.contains("longest turn 0.0ns"), "{line}");
}

#[gpui::test]
fn a_vault_with_no_notes_is_indexed_once_its_build_applies(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let (_fs, _notes, _links) = indexed_vault(cx, &[("image.png", "")]);
    cx.run_until_parked();

    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("(0 notes, corpus 0 bytes"), "{line}");
}

/// With no `last` snapshot, empty against empty is still the first build.
#[gpui::test]
fn a_vault_with_no_notes_applies_one_build(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("image.png", "")]);
    let (changed, _subscription) = notes_changed(cx, &links);
    cx.run_until_parked();
    assert_eq!(status(cx, &links), Status::Ready);

    rescan(cx, &fs, &notes);

    assert_eq!(changed.get(), 1);
}

#[gpui::test]
fn a_saves_turn_counts_in_the_longest_turn(cx: &mut TestAppContext) {
    let (_fs, notes, _links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));

    let a = entry_id(&notes, cx, "a");
    notes.update(cx, |_, cx| {
        cx.emit(NoteSaved {
            id: a,
            text: Arc::from("[[b]]"),
            stamp: Stamp {
                mtime: Mtime(SystemTime::UNIX_EPOCH),
                size: 5,
            },
        });
    });
    cx.run_until_parked();

    assert!(cx.read(|cx| cx.global::<Timeline>().longest_turn()) > Duration::ZERO);
}

#[gpui::test]
fn a_rescans_own_turn_counts_in_the_longest_turn(cx: &mut TestAppContext) {
    let (_fs, notes, _links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));

    // The build it starts applies only once the test parks.
    let vault = notes.read_with(cx, |notes, _| notes.vault().expect("a vault").clone());
    vault.update(cx, |_, cx| cx.notify());

    assert!(cx.read(|cx| cx.global::<Timeline>().longest_turn()) > Duration::ZERO);
    cx.run_until_parked();
}
