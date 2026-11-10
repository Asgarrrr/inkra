use std::sync::Arc;

use inkra_vault::Stamp;

use crate::test_support::{content, indexed, load, mtime};
use crate::{Backlink, Landed, LinkContext, Read, Slot};

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
            stale: false
        }
    );
}

#[test]
fn a_read_that_does_not_prove_the_ranges_has_no_context_and_goes_stale() {
    let reads = [
        text(SOURCE, SOURCE.len() + 1),
        Read::Gone,
        Read::NotUtf8,
        Read::Failed,
        text("[[n001]]", SOURCE.len()),
    ];
    for read in reads {
        assert_eq!(
            Slot::land(None, &backlink(), &read),
            Landed {
                slot: Slot::NoContext { of: backlink() },
                stale: true
            },
            "{read:?}"
        );
    }
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
    ];
    for (previous, stale) in previous {
        assert_eq!(
            Slot::land(Some(&previous), &reloaded, &Read::Gone),
            Landed {
                slot: no_context(&reloaded),
                stale
            },
            "{previous:?}"
        );
    }
}
