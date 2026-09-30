use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use proptest::prelude::*;

use super::{Ledger, NOTES_CHANGED_REFRESH, Run, SEARCH_DEBOUNCE, Started};

fn cancelled(started: &Started) -> bool {
    started.cancel.load(Ordering::Relaxed)
}

/// A run whose scan is in flight.
fn scan(ledger: &mut Ledger) -> Started {
    let started = ledger.start();
    ledger.begin_scan(&started);
    started
}

#[test]
fn a_start_cancels_the_run_before_and_supersedes_it() {
    let mut ledger = Ledger::default();
    let first = ledger.start();
    let second = ledger.start();
    assert!(cancelled(&first));
    assert!(!cancelled(&second));
    assert!(!ledger.is_current(first.id));
    assert!(ledger.is_current(second.id));
}

#[test]
#[should_panic(expected = "begin_scan on a superseded run")]
fn begin_scan_rejects_a_superseded_run() {
    let mut ledger = Ledger::default();
    let first = ledger.start();
    let _second = ledger.start();
    ledger.begin_scan(&first);
}

#[test]
fn dropping_the_ledger_cancels_its_run() {
    let mut ledger = Ledger::default();
    let started = scan(&mut ledger);
    drop(ledger);
    assert!(cancelled(&started));
}

#[test]
fn refreshes_during_a_scan_defer_to_one_rerun_and_never_cancel() {
    let mut ledger = Ledger::default();
    let started = scan(&mut ledger);
    for _ in 0..10 {
        assert_eq!(ledger.refresh(Run::Notify), None);
    }
    assert!(!cancelled(&started));
    assert_eq!(ledger.landed(), Some(Run::Notify));
    assert_eq!(ledger.landed(), None);
}

#[test]
fn a_refresh_with_no_scan_starts_at_once_as_the_next_run() {
    let mut ledger = Ledger::default();
    assert_eq!(ledger.refresh(Run::Refresh), Some(Run::Refresh));
    let started = ledger.start();
    assert_eq!(started.run, Run::Refresh);
    assert_eq!(started.wait, Some(NOTES_CHANGED_REFRESH));
    assert_eq!(ledger.start().run, Run::Search);
}

#[test]
fn a_start_during_a_scan_drops_its_rerun() {
    let mut ledger = Ledger::default();
    let _stale = scan(&mut ledger);
    assert_eq!(ledger.refresh(Run::Notify), None);
    let _newest = scan(&mut ledger);
    assert_eq!(ledger.landed(), None);
}

#[test]
fn a_keystroke_waits_only_behind_a_user_scan() {
    let mut ledger = Ledger::default();
    assert_eq!(ledger.start().wait, None, "idle");
    let _user = scan(&mut ledger);
    assert_eq!(
        ledger.start().wait,
        Some(SEARCH_DEBOUNCE),
        "behind a user scan"
    );
    assert_eq!(ledger.start().wait, None, "after a settle");
    assert_eq!(ledger.refresh(Run::Notify), Some(Run::Notify));
    let background = scan(&mut ledger);
    assert_eq!(background.run, Run::Notify);
    assert_eq!(ledger.start().wait, None, "behind a background scan");
}

#[test]
fn landing_ends_the_scan() {
    let mut ledger = Ledger::default();
    let _started = scan(&mut ledger);
    assert_eq!(ledger.landed(), None);
    assert_eq!(ledger.refresh(Run::Notify), Some(Run::Notify));
}

#[test]
fn the_rerun_waits_nothing() {
    let mut ledger = Ledger::default();
    let _started = scan(&mut ledger);
    assert_eq!(ledger.refresh(Run::Refresh), None);
    assert_eq!(ledger.landed(), Some(Run::Notify));
    assert_eq!(ledger.refresh(Run::Notify), Some(Run::Notify));
    assert_eq!(ledger.start().wait, None);
}

#[derive(Debug, Clone)]
enum Step {
    Start { scan: bool },
    Refresh(Run),
    Landed,
}

fn step() -> impl Strategy<Value = Step> {
    let run = prop_oneof![Just(Run::Search), Just(Run::Refresh), Just(Run::Notify)];
    prop_oneof![
        any::<bool>().prop_map(|scan| Step::Start { scan }),
        run.prop_map(Step::Refresh),
        Just(Step::Landed),
    ]
}

fn set(flags: &[Arc<AtomicBool>]) -> Vec<bool> {
    flags
        .iter()
        .map(|flag| flag.load(Ordering::Relaxed))
        .collect()
}

proptest! {
    #[test]
    fn only_the_newest_run_is_uncancelled(steps in prop::collection::vec(step(), 0..40)) {
        let mut ledger = Ledger::default();
        let mut flags: Vec<Arc<AtomicBool>> = Vec::new();
        let (mut in_flight, mut refreshed) = (false, false);
        for step in steps {
            match step {
                Step::Start { scan } => {
                    let started = ledger.start();
                    if scan {
                        ledger.begin_scan(&started);
                    }
                    flags.push(Arc::clone(&started.cancel));
                    (in_flight, refreshed) = (scan, false);
                }
                Step::Refresh(run) => {
                    let before = set(&flags);
                    let next = ledger.refresh(run);
                    prop_assert_eq!(set(&flags), before);
                    prop_assert_eq!(next, (!in_flight).then_some(run));
                    refreshed |= in_flight;
                }
                Step::Landed => {
                    if !in_flight {
                        continue;
                    }
                    prop_assert_eq!(ledger.landed().is_some(), refreshed);
                    (in_flight, refreshed) = (false, false);
                }
            }
            if let Some((newest, before)) = flags.split_last() {
                prop_assert!(!newest.load(Ordering::Relaxed));
                prop_assert!(before.iter().all(|flag| flag.load(Ordering::Relaxed)));
            }
        }
    }
}
