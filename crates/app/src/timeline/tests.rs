use std::time::{Duration, Instant};

use super::{Step, Timeline};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn indexed(notes: usize) -> Step {
    Step::Indexed {
        notes,
        corpus_bytes: 1_234,
        words_bytes: 567,
    }
}

#[test]
fn each_step_keeps_its_first_time() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    assert!(!timeline.record(Step::Root, start + ms(5)));
    assert!(!timeline.record(Step::Root, start + ms(9)));
    assert!(!timeline.record(Step::NamesReady, start + ms(20)));
    assert_eq!(
        timeline.line(),
        "launch: first frame -, root 5.0ms, scanned -, names ready 20.0ms, indexed -"
    );
}

#[test]
fn the_line_is_due_once_when_indexing_first_ends() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    timeline.record_turn(Duration::from_micros(1_500));
    assert!(!timeline.record(Step::FirstFrame, start + ms(40)));
    assert!(timeline.record(indexed(3), start + ms(900)));
    assert!(
        !timeline.record(indexed(4), start + ms(950)),
        "a later end logs nothing"
    );
    assert_eq!(
        timeline.line(),
        "launch: first frame 40.0ms, root -, scanned -, names ready -, \
         indexed 900.00ms (3 notes, corpus 1234 bytes, words 567 bytes, longest turn 1.5ms)"
    );
}

#[test]
fn indexed_before_first_frame_is_due_at_the_first_frame() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    timeline.record_turn(Duration::from_micros(1_500));
    assert!(!timeline.record(indexed(3), start + ms(30)), "no frame yet");
    assert!(timeline.record(Step::FirstFrame, start + ms(40)));
    assert!(
        !timeline.record(Step::FirstFrame, start + ms(60)),
        "a later frame logs nothing"
    );
    assert_eq!(
        timeline.line(),
        "launch: first frame 40.0ms, root -, scanned -, names ready -, \
         indexed 30.00ms (3 notes, corpus 1234 bytes, words 567 bytes, longest turn 1.5ms)"
    );
}

#[test]
fn the_longest_turn_counts_until_the_line_is_due() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    timeline.record_turn(Duration::from_micros(900));
    timeline.record_turn(Duration::from_micros(2_500));
    timeline.record_turn(ms(1));
    assert!(!timeline.record(Step::FirstFrame, start + ms(40)));
    assert!(timeline.record(indexed(3), start + ms(900)));
    timeline.record_turn(ms(50));
    assert!(
        timeline.line().ends_with("longest turn 2.5ms)"),
        "{}",
        timeline.line()
    );
}
