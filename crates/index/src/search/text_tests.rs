//! `TextSearch` over texts read now: read failures, the limit, the stale rules.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::{EntryId, FileIdentity, Loaded, Stamp, VaultError};

use crate::test_support::{indexed, mtime};
use crate::{Candidate, IndexSnapshot, NoteQuery, READ_FAILURES, Read, TextFound, TextSearch};

fn candidates(snapshot: &IndexSnapshot, query: &NoteQuery) -> Vec<Candidate> {
    let Some(candidates) = snapshot.candidates(query, &AtomicBool::new(false)) else {
        panic!("not cancelled");
    };
    candidates
}

/// Feeds `candidates` the reads `read` gives, in order, until the search wants no more.
fn drive(
    query: &NoteQuery,
    limit: usize,
    candidates: &[Candidate],
    read: impl Fn(usize, &Candidate) -> Read,
) -> TextFound {
    let cancel = AtomicBool::new(false);
    let mut search = TextSearch::new(query, limit);
    for (at, candidate) in candidates.iter().enumerate() {
        if !search.read(candidate, read(at, candidate), &cancel) {
            break;
        }
    }
    let Some(found) = search.finish(&cancel) else {
        panic!("not cancelled");
    };
    found
}

/// `text`, read with the stamp it was loaded with.
fn unchanged(text: &str, candidate: &Candidate) -> Read {
    Read::Text {
        text: text.into(),
        stamp: candidate.stamp,
    }
}

/// Each note with hits and how many, in order.
fn noted(found: &TextFound) -> Vec<(EntryId, usize)> {
    found
        .searched
        .notes
        .iter()
        .map(|group| (group.note, group.hits.len()))
        .collect()
}

/// `n` notes holding `alpha` once, and their candidates for it.
fn alphas(n: usize) -> (Vec<EntryId>, NoteQuery, Vec<Candidate>) {
    let (ids, index) = indexed(&vec![Some("alpha"); n]);
    let query = NoteQuery::new("alpha", None, &[]);
    let candidates = candidates(&index.snapshot(), &query);
    assert_eq!(candidates.len(), n);
    (ids, query, candidates)
}

#[test]
fn seven_failures_continue() {
    let (ids, query, candidates) = alphas(READ_FAILURES + 1);

    let found = drive(&query, 1_000, &candidates, |at, candidate| match at {
        0 => Read::Gone,
        at if at < READ_FAILURES => Read::Failed,
        _ => unchanged("alpha", candidate),
    });

    assert_eq!(found.reads, READ_FAILURES + 1);
    assert_eq!(found.stale, [ids[0]]);
    assert_eq!(noted(&found), [(ids[READ_FAILURES], 1)]);
}

#[test]
fn eighth_failure_stops_without_stale() {
    let (ids, query, candidates) = alphas(READ_FAILURES + 3);

    let found = drive(&query, 1_000, &candidates, |at, candidate| match at {
        0 => Read::Gone,
        1 => unchanged("alpha", candidate),
        at if at < READ_FAILURES + 2 => Read::Failed,
        _ => unchanged("alpha", candidate),
    });

    assert_eq!(
        found.reads,
        READ_FAILURES + 2,
        "the eighth failure is the last read"
    );
    assert!(found.stale.is_empty());
    assert_eq!(noted(&found), [(ids[1], 1)], "the hits so far still land");
    assert!(!found.searched.limit_reached);
}

#[test]
fn stops_at_the_thousandth_hit() {
    let (_, query, candidates) = alphas(1_001);

    let found = drive(&query, 1_000, &candidates, |_, candidate| {
        unchanged("alpha", candidate)
    });

    assert_eq!(
        noted(&found).iter().map(|(_, hits)| hits).sum::<usize>(),
        1_000
    );
    assert!(found.searched.limit_reached);
    assert_eq!(found.reads, 1_000);
}

#[test]
fn the_last_note_fills_only_the_room_left() {
    let (ids, query, candidates) = alphas(2);

    let found = drive(&query, 3, &candidates, |_, candidate| {
        unchanged("alpha alpha", candidate)
    });

    assert_eq!(noted(&found), [(ids[0], 2), (ids[1], 1)]);
    assert!(found.searched.limit_reached);
}

#[test]
fn stale_rules() {
    let (ids, query, candidates) = alphas(6);
    let changed = |candidate: &Candidate, size: u64, at: u64| Stamp {
        mtime: mtime(at),
        size: candidate.stamp.size + size,
    };

    let found = drive(&query, 1_000, &candidates, |at, candidate| match at {
        0 => unchanged("alpha", candidate),
        1 => Read::Text {
            text: "alpha alpha".into(),
            stamp: changed(candidate, 1, 0),
        },
        2 => Read::Text {
            text: "beta".into(),
            stamp: changed(candidate, 0, 9),
        },
        3 => Read::Gone,
        4 => Read::NotUtf8,
        _ => Read::Failed,
    });

    assert_eq!(
        found.stale,
        ids[1..5],
        "rewritten, gone, not UTF-8; never a failure"
    );
    assert_eq!(
        noted(&found),
        [(ids[0], 1), (ids[1], 2)],
        "a rewritten text is searched as read"
    );
    assert_eq!(found.reads, 6);
}

#[test]
fn hits_counts_the_hits_so_far() {
    let (_, query, candidates) = alphas(2);
    let cancel = AtomicBool::new(false);
    let mut search = TextSearch::new(&query, 1_000);
    assert_eq!(search.hits(), 0);

    assert!(search.read(&candidates[0], unchanged("alpha", &candidates[0]), &cancel));
    assert!(search.read(
        &candidates[1],
        unchanged("alpha alpha", &candidates[1]),
        &cancel
    ));

    assert_eq!(search.hits(), 3);
}

#[test]
fn a_cancelled_search_stops_and_finds_nothing() {
    let (_, query, candidates) = alphas(2);
    let cancel = AtomicBool::new(false);
    let mut search = TextSearch::new(&query, 1_000);
    cancel.store(true, Ordering::Relaxed);

    assert!(!search.read(&candidates[0], unchanged("alpha", &candidates[0]), &cancel));
    assert!(
        !search.read(&candidates[1], Read::Gone, &cancel),
        "a gone note after the cancel is not folded"
    );
    assert!(search.finish(&cancel).is_none());
}

#[test]
fn a_load_maps_to_its_read() {
    let stamp = Stamp {
        mtime: mtime(1),
        size: 5,
    };
    let loaded = Loaded {
        text: "alpha".to_owned(),
        stamp,
        identity: FileIdentity {
            device: 1,
            inode: 1,
        },
    };
    let failed = |kind| {
        Err(VaultError::Io {
            path: "a.md".into(),
            source: io::Error::from(kind),
        })
    };

    let text = Read::Text {
        text: "alpha".into(),
        stamp,
    };
    assert_eq!(Read::from(Ok(loaded)), text);
    assert_eq!(Read::from(failed(io::ErrorKind::NotFound)), Read::Gone);
    let not_utf8 = Err(VaultError::NotUtf8 {
        path: "a.md".into(),
    });
    assert_eq!(Read::from(not_utf8), Read::NotUtf8);
    assert_eq!(
        Read::from(failed(io::ErrorKind::PermissionDenied)),
        Read::Failed
    );
}
