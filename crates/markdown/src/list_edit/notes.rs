//! Generated notes: lists of bullets, numbers up to three digits, tasks, quotes, tabs in the
//! indent, held lines, lazy lines and code blocks in items, nesting up to four.

use proptest::prelude::*;

use super::oracle::oracle;
use crate::spans::line_end;

/// A generated note's line, before its quote prefix.
#[derive(Debug, Clone)]
enum Row {
    /// An item at a nesting level, its marker, its box and its text.
    Item(usize, &'static str, &'static str, &'static str),
    /// A line held at a nesting level, past its item's content column.
    Held(usize, &'static str),
    /// A lazy line: no indent, no quote prefix.
    Lazy(&'static str),
    Blank,
    /// A fenced code block held at a nesting level.
    Code(usize),
}

/// Every marker the notes use.
const MARKERS: [&str; 8] = ["-", "*", "1.", "9.", "10.", "99.", "1)", "123)"];

fn row(markers: &[&'static str]) -> impl Strategy<Value = Row> {
    let text = prop::sample::select(vec!["a", "b c", ""]);
    let marker = prop::sample::select(markers.to_vec());
    let task = prop::sample::select(vec!["", "", "[ ] ", "[x] "]);
    prop_oneof![
        4 => (0..4usize, marker, task, text.clone())
            .prop_map(|(level, marker, task, text)| Row::Item(level, marker, task, text)),
        2 => (1..4usize, text.clone()).prop_map(|(level, text)| Row::Held(level, text)),
        1 => prop::sample::select(vec!["z", "y x"]).prop_map(Row::Lazy),
        1 => Just(Row::Blank),
        1 => (1..4usize).prop_map(Row::Code),
    ]
}

/// The note `rows` write in `quotes` quotes, a tab for each indent's first four columns when
/// `tabs`, and each item's depth as written; each item's level is cut to one past the levels open.
fn note(rows: &[Row], quotes: usize, tabs: bool) -> (String, Vec<usize>) {
    let quote = "> ".repeat(quotes);
    let blank = quote.trim_end();
    // Each open level's content column, past the quote prefix.
    let mut columns: Vec<usize> = Vec::new();
    let indent = |column: usize| {
        if tabs && quotes == 0 && column >= 4 {
            format!("\t{:1$}", "", column - 4)
        } else {
            " ".repeat(column)
        }
    };
    let (mut lines, mut depths) = (Vec::new(), Vec::new());
    let mut after_empty = false;
    for row in rows {
        // An empty item ends at a blank line (`CommonMark` 5.2).
        if after_empty && matches!(row, Row::Blank) {
            columns.pop();
        }
        after_empty = matches!(row, Row::Item(.., ""));
        match *row {
            Row::Item(level, marker, task, text) => {
                columns.truncate(level.min(columns.len()));
                let at = columns.last().copied().unwrap_or(0);
                depths.push(columns.len());
                lines.push(format!("{quote}{}{marker} {task}{text}", indent(at)));
                columns.push(at + marker.len() + 1);
            }
            Row::Held(level, text) if !columns.is_empty() && !text.is_empty() => {
                columns.truncate(level.min(columns.len()));
                let at = columns[columns.len() - 1];
                lines.push(format!("{quote}{}{text}", indent(at)));
            }
            Row::Lazy(text) => lines.push(text.to_owned()),
            Row::Code(level) if !columns.is_empty() => {
                columns.truncate(level.min(columns.len()));
                let at = indent(columns[columns.len() - 1]);
                lines.extend([
                    format!("{quote}{at}```"),
                    format!("{quote}{at}x"),
                    format!("{quote}{at}```"),
                ]);
            }
            _ => lines.push(blank.to_owned()),
        }
    }
    (lines.join("\n"), depths)
}

/// A note that parses as it was written, and a caret in it: on a chosen line, at its end or at a
/// chosen byte. A marker that cannot interrupt a paragraph, or a sibling indented under an empty
/// item, reads as other than written; such a note is rejected.
pub(super) fn case() -> impl Strategy<Value = (String, usize)> {
    case_of(&MARKERS)
}

/// A note as `case` makes them, with bullet lists alone.
pub(super) fn bullet_case() -> impl Strategy<Value = (String, usize)> {
    case_of(&MARKERS[..2])
}

fn case_of(markers: &[&'static str]) -> impl Strategy<Value = (String, usize)> {
    (
        prop::collection::vec(row(markers), 1..8),
        0..3usize,
        any::<bool>(),
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        any::<bool>(),
    )
        .prop_filter_map(
            "parses as written",
            |(rows, quotes, tabs, line, byte, at_end)| {
                let (text, depths) = note(&rows, quotes, tabs);
                let read: Vec<usize> = oracle(&text).0.iter().map(|item| item.depth).collect();
                if read != depths {
                    return None;
                }
                let starts: Vec<usize> = std::iter::once(0)
                    .chain(text.match_indices('\n').map(|(at, _)| at + 1))
                    .collect();
                let start = starts[line.index(starts.len())];
                let end = line_end(&text, start);
                let caret = if at_end {
                    end
                } else {
                    start + byte.index(end - start + 1)
                };
                Some((text, caret))
            },
        )
}
