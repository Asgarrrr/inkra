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
        names_bytes: 89,
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
        "launch: first frame -, session read -, root 5.0ms, scanned -, names ready 20.0ms, cache -, seed turn -, indexed -"
    );
}

#[test]
fn the_line_shows_how_long_the_session_read_took() {
    let mut timeline = Timeline::new(Instant::now());
    timeline.record_session_read(Duration::from_micros(250));
    assert_eq!(
        timeline.line(),
        "launch: first frame -, session read 250.00µs, root -, scanned -, names ready -, cache -, seed turn -, indexed -"
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
        "launch: first frame 40.0ms, session read -, root -, scanned -, names ready -, cache -, \
         seed turn -, indexed 900.00ms (3 notes, corpus 1234 bytes, words 567 bytes, names 89 bytes, longest turn 1.5ms)"
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
        "launch: first frame 40.0ms, session read -, root -, scanned -, names ready -, cache -, \
         seed turn -, indexed 30.00ms (3 notes, corpus 1234 bytes, words 567 bytes, names 89 bytes, longest turn 1.5ms)"
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

#[test]
fn the_line_names_the_cache_outcome_at_its_first_time() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    assert!(!timeline.record(Step::Cached("decoded"), start + ms(12)));
    assert!(!timeline.record(Step::Cached("missing"), start + ms(30)));
    assert_eq!(
        timeline.line(),
        "launch: first frame -, session read -, root -, scanned -, names ready -, \
         cache decoded 12.0ms, seed turn -, indexed -"
    );
}

#[test]
fn the_line_shows_the_first_seed_turn() {
    let mut timeline = Timeline::new(Instant::now());
    timeline.record_seed_turn(Duration::from_micros(4_250));
    timeline.record_seed_turn(ms(9));
    assert_eq!(
        timeline.line(),
        "launch: first frame -, session read -, root -, scanned -, names ready -, cache -, \
         seed turn 4.25ms, indexed -"
    );
}

#[gpui::test]
fn the_session_read_reaches_the_global_timeline(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Timeline::new(Instant::now()));
        Timeline::session_read(Duration::from_micros(250), cx);
        let line = cx.global::<Timeline>().line();
        assert!(line.contains("session read 250.00µs"), "{line}");
    });
}
