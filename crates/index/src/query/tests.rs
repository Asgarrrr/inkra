use std::sync::atomic::{AtomicBool, Ordering};

use futures::executor::block_on;
use inkra_vault::{EntryId, Job, Jobs, Serial, Stamp};
use proptest::prelude::*;

use crate::find::model_cases::{self, Built, Case, build};
use crate::find::{find_in, merge_found};
use crate::search::search_in;
use crate::test_support::{id, indexed, load, mtime, note_set, vault};
use crate::{IndexSnapshot, NoteIndex, NoteMatch, NoteQuery, Searched};

/// `n` ranges, their jobs run inline in order.
struct Ranges(usize);

impl Jobs for Ranges {
    fn parallelism(&self) -> usize {
        self.0
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Serial.run(jobs)
    }
}

/// Sets its flag once the first job ran: a cancel landing between two ranges.
struct CancelsAfterOne<'c>(&'c AtomicBool);

impl Jobs for CancelsAfterOne<'_> {
    fn parallelism(&self) -> usize {
        3
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            for (at, job) in jobs.into_iter().enumerate() {
                if at == 1 {
                    self.0.store(true, Ordering::Relaxed);
                }
                job.await;
            }
        })
    }
}

fn not_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

/// The one-range answer the entries must equal.
fn found_in_one_range(snapshot: &IndexSnapshot, query: &NoteQuery, limit: usize) -> Vec<NoteMatch> {
    let cancel = not_cancelled();
    let Some(lookup) = snapshot.lookup(query, &cancel) else {
        panic!("not cancelled");
    };
    let all = 0..snapshot.names().len();
    let kept = find_in(snapshot, &lookup, all, query, &cancel, limit);
    merge_found(snapshot, vec![kept], query, limit)
}

fn searched_in_one_range(snapshot: &IndexSnapshot, query: &NoteQuery, limit: usize) -> Searched {
    let cancel = not_cancelled();
    let Some(lookup) = snapshot.lookup(query, &cancel) else {
        panic!("not cancelled");
    };
    search_in(
        snapshot,
        &lookup,
        0..snapshot.names().len(),
        query,
        &cancel,
        limit,
    )
}

fn any_split_equals_one_range(
    case: &Case,
    parallelism: usize,
    limit: usize,
) -> Result<(), TestCaseError> {
    let Built { index, query, .. } = build(case);
    let snapshot = index.snapshot();
    let jobs = Ranges(parallelism);
    let found = block_on(snapshot.find_notes(&query, limit, &jobs, &not_cancelled()));
    prop_assert_eq!(found, Some(found_in_one_range(&snapshot, &query, limit)));
    let searched = block_on(snapshot.search(&query, limit, &jobs, &not_cancelled()));
    prop_assert_eq!(
        searched,
        Some(searched_in_one_range(&snapshot, &query, limit))
    );
    Ok(())
}

proptest! {
    #[test]
    fn any_parallelism_answers_as_one_range(
        case in model_cases::case(),
        parallelism in 1..=8_usize,
        limit in 0..10_usize,
    ) {
        any_split_equals_one_range(&case, parallelism, limit)?;
    }
}

#[test]
fn merging_the_snapshot_s_chunks_equals_one_range() {
    let files: Vec<String> = (0..40).map(|n| format!("f{}/plan {n}.md", n % 7)).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    for file in &files {
        load(&mut index, id(&vault, file), "");
    }
    let snapshot = index.snapshot();
    let query = NoteQuery::new("plan 3", None, &[]);
    let found = block_on(snapshot.find_notes(&query, 5, &Ranges(4), &not_cancelled()));
    assert_eq!(found.as_ref().map(Vec::len), Some(5));
    assert_eq!(found, Some(found_in_one_range(&snapshot, &query, 5)));
}

#[test]
fn merge_concatenates_in_range_order_and_truncates() {
    let (_, index) = indexed(&[Some("a a"), Some("a a"), Some("a a")]);
    let snapshot = index.snapshot();
    let query = NoteQuery::new("a", None, &[]);
    let searched = block_on(snapshot.search(&query, 5, &Ranges(3), &not_cancelled()));
    assert!(
        searched
            .as_ref()
            .is_some_and(|searched| searched.limit_reached)
    );
    assert_eq!(searched, Some(searched_in_one_range(&snapshot, &query, 5)));
}

#[test]
fn a_cancel_set_before_the_run_answers_nothing() {
    let (_, index) = indexed(&[Some("plan"), Some("plan")]);
    let snapshot = index.snapshot();
    let query = NoteQuery::new("plan", None, &[]);
    let cancel = AtomicBool::new(true);
    assert_eq!(
        block_on(snapshot.find_notes(&query, 10, &Serial, &cancel)),
        None
    );
    assert_eq!(
        block_on(snapshot.search(&query, 10, &Serial, &cancel)),
        None
    );
}

#[test]
fn a_cancel_set_during_the_run_answers_nothing_not_a_part() {
    let (_, index) = indexed(&[Some("plan"), Some("plan"), Some("plan")]);
    let snapshot = index.snapshot();
    let query = NoteQuery::new("plan", None, &[]);
    let cancel = not_cancelled();
    let found = block_on(snapshot.find_notes(&query, 10, &CancelsAfterOne(&cancel), &cancel));
    assert_eq!(found, None);
    let cancel = not_cancelled();
    let searched = block_on(snapshot.search(&query, 10, &CancelsAfterOne(&cancel), &cancel));
    assert_eq!(searched, None);
}

#[test]
fn an_empty_vault_answers_empty_not_nothing() {
    let snapshot = NoteIndex::default().snapshot();
    let query = NoteQuery::new("plan", None, &[]);
    let found = block_on(snapshot.find_notes(&query, 10, &Ranges(4), &not_cancelled()));
    assert_eq!(found, Some(Vec::new()));
    let searched = block_on(snapshot.search(&query, 10, &Ranges(4), &not_cancelled()));
    assert_eq!(searched, Some(Searched::default()));
}

#[test]
fn candidates_are_the_notes_holding_every_term_in_vault_order() {
    let texts: Vec<Option<&str>> = (0..300)
        .map(|n| Some(if n % 2 == 0 { "alpha beta" } else { "alpha" }))
        .collect();
    let (ids, index) = indexed(&texts);
    let query = NoteQuery::new("alpha beta", None, &[]);

    let Some(candidates) = index.snapshot().candidates(&query, &AtomicBool::new(false)) else {
        panic!("not cancelled");
    };

    let want: Vec<(EntryId, String, Stamp)> = (0..300)
        .step_by(2)
        .map(|n| {
            let stamp = Stamp {
                mtime: mtime(0),
                size: 10,
            };
            (ids[n], format!("n{n:03}.md"), stamp)
        })
        .collect();
    let got: Vec<(EntryId, String, Stamp)> = candidates
        .into_iter()
        .map(|candidate| (candidate.id, candidate.path.to_string(), candidate.stamp))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn candidates_stop_once_cancelled() {
    let (_, index) = indexed(&[Some("alpha")]);
    let query = NoteQuery::new("alpha", None, &[]);

    assert!(
        index
            .snapshot()
            .candidates(&query, &AtomicBool::new(true))
            .is_none()
    );
}
