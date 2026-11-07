use std::time::{Duration, Instant};

use super::{Due, QUIET, Writes};

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
