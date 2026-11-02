use std::ops::Range;

use gpui::{FontWeight, Hsla, TextStyle};
use inkra_markdown::{DisplayLine, parse};
use inkra_text::ByteOffset;

use super::{RunStyles, line_runs};
use crate::theme;

fn range(start: usize, end: usize) -> Range<ByteOffset> {
    ByteOffset::new(start)..ByteOffset::new(end)
}

fn lens(runs: &[gpui::TextRun]) -> Vec<usize> {
    runs.iter().map(|run| run.len).collect()
}

#[test]
fn runs_follow_the_spans_and_cover_the_line() {
    let text = "a **b** c";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let line = range(0, text.len());
    let display = DisplayLine::new(line.clone(), []);

    let runs = line_runs(&line, &spans, None, &display, &mut styles);

    assert_eq!(lens(&runs), vec![2, 2, 1, 2, 2]);
    assert_eq!(runs[0].font.weight, base.font_weight);
    assert_eq!(runs[1].color, Hsla::from(theme::marker_text()));
    assert_eq!(runs[2].font.weight, FontWeight::BOLD);
    assert!(runs.iter().all(|run| run.underline.is_none()));
}

#[test]
fn a_composition_inside_bold_stays_bold_and_gains_the_underline() {
    let text = "a **b** c";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let line = range(0, text.len());
    let display = DisplayLine::new(line.clone(), []);

    let runs = line_runs(&line, &spans, Some(&range(3, 6)), &display, &mut styles);

    assert_eq!(lens(&runs), vec![2, 1, 1, 1, 1, 1, 2]);
    let underlined: Vec<bool> = runs.iter().map(|run| run.underline.is_some()).collect();
    assert_eq!(
        underlined,
        vec![false, false, true, true, true, false, false]
    );
    assert_eq!(runs[3].font.weight, FontWeight::BOLD);
}

#[test]
fn runs_are_indexed_from_the_line_start() {
    let text = "ab\n**c**";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let line = range(3, 8);
    let display = DisplayLine::new(line.clone(), []);

    let runs = line_runs(&line, &spans[..], None, &display, &mut styles);

    assert_eq!(lens(&runs), vec![2, 1, 2]);
}

#[test]
fn a_plain_line_is_one_run_and_an_empty_line_one_empty_run() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    for (line, expected) in [(range(0, 5), vec![5]), (range(4, 4), vec![0])] {
        let display = DisplayLine::new(line.clone(), []);
        assert_eq!(
            lens(&line_runs(&line, &[], None, &display, &mut styles)),
            expected
        );
    }
}

#[test]
fn a_stale_span_past_the_line_is_clipped_not_fatal() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);
    let spans = parse("**long bold text**").spans;

    let line = range(0, 4);
    let display = DisplayLine::new(line.clone(), []);

    let runs = line_runs(&line, &spans, None, &display, &mut styles);

    assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 4);
}
