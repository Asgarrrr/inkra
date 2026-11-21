use std::ops::Range;

use gpui::{FontWeight, Hsla, TextStyle};
use inkra_editor::{HeardLayer, Moved, RevisionLog};
use inkra_markdown::{DisplayLine, parse};
use inkra_text::{Buffer, ByteOffset};

use super::{RunStyles, line_runs};
use crate::decorations::{Drawn, MarkStyle};
use crate::theme::ColorToken;
use crate::theme::builtin::dark;

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
    let mut styles = RunStyles::new(&base, dark(), &Drawn::default());

    let line = range(0, text.len());
    let display = DisplayLine::new(range(0, line.end - line.start), []);

    let runs = line_runs(&line, &spans, None, &display, &mut styles);

    assert_eq!(lens(&runs), vec![2, 2, 1, 2, 2]);
    assert_eq!(runs[0].font.weight, base.font_weight);
    assert_eq!(runs[1].color, Hsla::from(dark().colors.md_marker));
    assert_eq!(runs[2].font.weight, FontWeight::BOLD);
    assert!(runs.iter().all(|run| run.underline.is_none()));
}

#[test]
fn a_composition_inside_bold_stays_bold_and_gains_the_underline() {
    let text = "a **b** c";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base, dark(), &Drawn::default());

    let line = range(0, text.len());
    let display = DisplayLine::new(range(0, line.end - line.start), []);

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
    let mut styles = RunStyles::new(&base, dark(), &Drawn::default());

    let line = range(3, 8);
    let display = DisplayLine::new(range(0, line.end - line.start), []);

    let runs = line_runs(&line, &spans[..], None, &display, &mut styles);

    assert_eq!(lens(&runs), vec![2, 1, 2]);
}

#[test]
fn a_plain_line_is_one_run_and_an_empty_line_one_empty_run() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base, dark(), &Drawn::default());

    for (line, expected) in [(range(0, 5), vec![5]), (range(4, 4), vec![0])] {
        let display = DisplayLine::new(range(0, line.end - line.start), []);
        assert_eq!(
            lens(&line_runs(&line, &[], None, &display, &mut styles)),
            expected
        );
    }
}

#[test]
fn a_stale_span_past_the_line_is_clipped_not_fatal() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base, dark(), &Drawn::default());
    let spans = parse("**long bold text**").spans;

    let line = range(0, 4);
    let display = DisplayLine::new(range(0, line.end - line.start), []);

    let runs = line_runs(&line, &spans, None, &display, &mut styles);

    assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 4);
}

/// One layer over `text` holding `marks`, published at its edit mark.
fn layer(text: &Buffer, marks: Vec<(Range<ByteOffset>, MarkStyle)>) -> HeardLayer<MarkStyle> {
    let mut layer = HeardLayer::new(text.edit_mark());
    let chunk = (ByteOffset::new(0)..text.len(), 0, marks);
    let published = layer.publish(text, Vec::new(), vec![chunk], text.edit_mark());
    assert!(matches!(published, Some(Moved::Range(..))));
    layer
}

fn colored(color: ColorToken) -> MarkStyle {
    MarkStyle { color: Some(color) }
}

/// Whether each run of `text`'s one line, with `marked` composing, is in the warning colour.
fn warning_runs(
    text: &str,
    marked: Option<&Range<ByteOffset>>,
    layers: &[HeardLayer<MarkStyle>],
) -> Vec<bool> {
    let buffer = Buffer::new(text);
    let base = TextStyle::default();
    let log = RevisionLog::new();
    let drawn = Drawn::for_tests(layers, &log, buffer.edit_mark());
    let mut styles = RunStyles::new(&base, dark(), &drawn);
    let line = range(0, text.len());
    let display = DisplayLine::new(range(0, line.end - line.start), []);
    let warning = Hsla::from(dark().colors.warning);
    line_runs(&line, &parse(text).spans, marked, &display, &mut styles)
        .iter()
        .map(|run| run.color == warning)
        .collect()
}

#[test]
fn a_revealed_marker_wins_over_a_layer() {
    let text = "a **b** c";
    let layers = [layer(
        &Buffer::new(text),
        vec![(range(0, text.len()), colored(ColorToken::Warning))],
    )];

    assert_eq!(
        warning_runs(text, None, &layers),
        [true, false, true, false, true]
    );
}

#[test]
fn marked_text_wins_over_a_layer() {
    let text = "a b c";
    let layers = [layer(
        &Buffer::new(text),
        vec![(range(0, text.len()), colored(ColorToken::Warning))],
    )];

    assert_eq!(
        warning_runs(text, Some(&range(2, 3)), &layers),
        [true, false, true]
    );
}

/// A seed can re-split a mark without changing a colour, and logs nothing; the line it carries
/// must still match a fresh one.
#[test]
fn adjacent_marks_of_one_colour_make_one_run() {
    let buffer = Buffer::new("abcdef");
    let warning = colored(ColorToken::Warning);
    let layers = [layer(
        &buffer,
        vec![(range(0, 1), warning), (range(1, 5), warning)],
    )];
    let log = RevisionLog::new();
    let drawn = Drawn::for_tests(&layers, &log, buffer.edit_mark());
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base, dark(), &drawn);
    let display = DisplayLine::new(range(0, 6), []);

    let runs = line_runs(&range(0, 6), &[], None, &display, &mut styles);

    assert_eq!(lens(&runs), vec![5, 1]);
}

#[test]
fn a_layer_without_a_colour_leaves_the_one_below() {
    let text = "a b c";
    let buffer = Buffer::new(text);
    let layers = [
        layer(
            &buffer,
            vec![(range(0, text.len()), colored(ColorToken::Warning))],
        ),
        layer(&buffer, vec![(range(2, 3), MarkStyle::default())]),
    ];

    assert!(
        warning_runs(text, None, &layers)
            .iter()
            .all(|&warning| warning)
    );
}
