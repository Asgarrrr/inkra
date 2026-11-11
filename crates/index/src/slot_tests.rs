use std::sync::Arc;

use inkra_vault::Stamp;

use crate::test_support::{content, indexed, load, mtime};
use crate::{Backlink, Failures, Landed, LinkContext, READ_FAILURES, Read, Slot};

const SOURCE: &str = "[[n001]] and [[n001|x]]";

/// `SOURCE`'s one backlink to an empty note, loaded at `mtime(0)`.
fn backlink() -> Backlink {
    let (ids, index) = indexed(&[Some(SOURCE), Some("")]);
    let mut backlinks = index.backlinks(ids[1]);
    assert_eq!(backlinks.len(), 1, "one source links to n001");
    backlinks.remove(0)
}

fn text(text: &str, size: usize) -> Read {
    Read::Text {
        text: Arc::from(text),
        stamp: Stamp {
            mtime: mtime(0),
            size: size as u64,
        },
    }
}

#[test]
fn a_text_at_the_stamp_gives_each_occurrence_its_context_in_order() {
    let context = |link| LinkContext {
        text: SOURCE.into(),
        link,
    };
    assert_eq!(
        Slot::land(None, &backlink(), &text(SOURCE, SOURCE.len())),
        Landed {
            slot: Slot::Read {
                of: backlink(),
                contexts: vec![context(0..8), context(13..23)]
            },
            stale: false,
            forget: false
        }
    );
}

/// Only a text at the stamp disproves the ranges, so only it drops the cache's text.
#[test]
fn a_read_that_does_not_prove_the_ranges_has_no_context_and_goes_stale() {
    let reads = [
        (text(SOURCE, SOURCE.len() + 1), false),
        (Read::Gone, false),
        (Read::NotUtf8, false),
        (text("[[n001]]", SOURCE.len()), true),
        (text(&"x".repeat(SOURCE.len()), SOURCE.len()), true),
    ];
    for (read, forget) in reads {
        assert_eq!(
            Slot::land(None, &backlink(), &read),
            Landed {
                slot: Slot::NoContext { of: backlink() },
                stale: true,
                forget
            },
            "{read:?}"
        );
    }
}

#[test]
fn a_failed_read_holds_at_its_stamp_and_goes_not_stale() {
    let (ids, mut index) = indexed(&[Some(SOURCE), Some("")]);
    let of = index.backlinks(ids[1]).remove(0);
    let landed = Slot::land(None, &of, &Read::Failed);
    let failed = Slot::Failed {
        stamp: of.candidate().stamp,
    };
    assert_eq!(
        landed,
        Landed {
            slot: failed.clone(),
            stale: false,
            forget: false
        }
    );
    load(&mut index, ids[0], SOURCE);
    assert!(failed.is_valid(&index.backlinks(ids[1]).remove(0)));
    index.insert(ids[0], content(SOURCE, mtime(1)));
    assert!(!failed.is_valid(&index.backlinks(ids[1]).remove(0)));
}

#[test]
fn a_slot_is_valid_only_for_the_backlink_it_was_read_for() {
    let (ids, mut index) = indexed(&[Some(SOURCE), Some("")]);
    let of = index.backlinks(ids[1]).remove(0);
    load(&mut index, ids[0], SOURCE);
    let reloaded = index.backlinks(ids[1]).remove(0);
    for read in [text(SOURCE, SOURCE.len()), Read::Gone] {
        let slot = Slot::land(None, &of, &read).slot;
        assert!(slot.is_valid(&of), "{slot:?}");
        assert!(!slot.is_valid(&reloaded), "{slot:?}");
    }
}

/// The previous slot is the map's, invalid once the reload's new slice replaced its backlink.
#[test]
fn a_source_goes_stale_once_per_stamp() {
    let (ids, mut index) = indexed(&[Some(SOURCE), Some("")]);
    let before = index.backlinks(ids[1]).remove(0);
    load(&mut index, ids[0], SOURCE);
    let reloaded = index.backlinks(ids[1]).remove(0);
    index.insert(ids[0], content(SOURCE, mtime(1)));
    let touched = index.backlinks(ids[1]).remove(0);
    let no_context = |of: &Backlink| Slot::NoContext { of: of.clone() };
    let previous = [
        (no_context(&before), false),
        (no_context(&touched), true),
        (
            Slot::land(None, &before, &text(SOURCE, SOURCE.len())).slot,
            true,
        ),
        (Slot::land(None, &before, &Read::Failed).slot, true),
    ];
    for (previous, stale) in previous {
        assert_eq!(
            Slot::land(Some(&previous), &reloaded, &Read::Gone),
            Landed {
                slot: no_context(&reloaded),
                stale,
                forget: false
            },
            "{previous:?}"
        );
    }
}

#[test]
fn the_failures_stop_at_the_eighth_failed_slot() {
    let mut failures = Failures::default();
    let read = Slot::land(None, &backlink(), &text(SOURCE, SOURCE.len())).slot;
    let no_context = Slot::NoContext { of: backlink() };
    let failed = Slot::land(None, &backlink(), &Read::Failed).slot;
    for slot in [&read, &no_context].repeat(READ_FAILURES) {
        failures.count(slot);
    }
    for _ in 1..READ_FAILURES {
        failures.count(&failed);
    }
    assert!(!failures.stopped());
    failures.count(&failed);
    assert!(failures.stopped());
}
