use std::time::{Duration, Instant};

use super::{CHANGES, Due, MAX_BACKOFF, QUIET, Writes};

#[test]
fn the_revision_written_is_idle_and_the_next_waits_the_quiet_window() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(5));

    assert_eq!(writes.due(5, true, t0), Due::Idle);
    assert_eq!(writes.due(6, true, t0), Due::At(t0 + QUIET));
}

#[test]
fn the_first_call_writes_a_revision_the_file_lacks() {
    let t0 = Instant::now();
    for written in [Some(4), None] {
        assert_eq!(
            Writes::new(written).due(5, true, t0),
            Due::Now,
            "{written:?}"
        );
    }
}

#[test]
fn a_write_is_due_once_the_quiet_window_has_passed() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(5));
    let _ = writes.due(5, true, t0);
    let _ = writes.due(6, true, t0);

    assert_eq!(
        writes.due(6, true, t0 + QUIET.saturating_sub(Duration::from_millis(1))),
        Due::At(t0 + QUIET)
    );
    assert_eq!(writes.due(6, true, t0 + QUIET), Due::Now);
}

#[test]
fn a_change_restarts_the_quiet_window() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(5));
    let _ = writes.due(5, true, t0);
    let _ = writes.due(6, true, t0);

    let later = t0 + Duration::from_secs(30);
    assert_eq!(writes.due(7, true, later), Due::At(later + QUIET));
}

#[test]
fn nothing_is_due_until_ready_and_the_first_call_is_spent_anyway() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);

    assert_eq!(writes.due(5, false, t0), Due::Idle);
    assert_eq!(writes.due(5, true, t0), Due::At(t0 + QUIET));
}

#[test]
fn a_started_revision_is_idle() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(5));
    let _ = writes.due(5, true, t0);
    let _ = writes.due(6, true, t0);

    writes.started(6);

    assert_eq!(writes.due(6, true, t0 + QUIET), Due::Idle);
}

#[test]
fn the_500th_change_since_the_file_is_due_at_once() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(10));
    let _ = writes.due(10, true, t0);

    assert_eq!(writes.due(9 + CHANGES, true, t0), Due::At(t0 + QUIET));
    assert_eq!(writes.due(10 + CHANGES, true, t0), Due::Now);
}

#[test]
fn the_500_changes_count_from_the_revision_started() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);
    let _ = writes.due(1, true, t0);
    writes.started(20);

    assert_eq!(writes.due(19 + CHANGES, true, t0), Due::At(t0 + QUIET));
    assert_eq!(writes.due(20 + CHANGES, true, t0), Due::Now);
}

#[test]
fn a_failed_write_retries_after_the_quiet_window() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);
    writes.started(6);
    writes.failed(6, t0);

    assert_eq!(
        writes.due(6, true, t0 + QUIET.saturating_sub(Duration::from_millis(1))),
        Due::At(t0 + QUIET)
    );
    assert_eq!(writes.due(6, true, t0 + QUIET), Due::Now);
}

#[test]
fn each_failure_doubles_the_backoff_up_to_ten_minutes() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);
    let _ = writes.due(6, true, t0);

    let delays: Vec<u64> = (0..6)
        .map(|_| {
            writes.started(6);
            writes.failed(6, t0);
            match writes.due(6, true, t0) {
                Due::At(retry) => (retry - t0).as_secs(),
                due => panic!("{due:?}"),
            }
        })
        .collect();

    assert_eq!(delays, [60, 120, 240, 480, 600, 600]);
}

#[test]
fn a_backoff_holds_past_500_changes() {
    let t0 = Instant::now();
    let mut writes = Writes::new(Some(0));
    let _ = writes.due(0, true, t0);
    writes.started(6);
    writes.failed(6, t0);

    assert_eq!(writes.due(6 + CHANGES + 100, true, t0), Due::At(t0 + QUIET));
}

#[test]
fn a_landed_write_clears_the_backoff() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);
    writes.started(6);
    writes.failed(6, t0);
    writes.wrote();
    writes.started(7);
    writes.failed(7, t0);

    assert_eq!(
        writes.due(7, true, t0),
        Due::At(t0 + QUIET),
        "the delay starts over"
    );
}

#[test]
fn an_older_failure_keeps_the_newer_revision_written() {
    let t0 = Instant::now();
    let mut writes = Writes::new(None);
    let _ = writes.due(5, true, t0);
    writes.started(6);
    writes.started(7);
    writes.failed(6, t0);

    assert_eq!(writes.due(7, true, t0 + MAX_BACKOFF), Due::Idle);
}
