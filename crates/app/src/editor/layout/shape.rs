use std::ops::Range;

use gpui::{Pixels, SharedString, TextStyle, Window};
use inkra_markdown::{DisplayLine, Markdown, is_active};
use inkra_text::{Buffer, ByteOffset};

use super::runs::{RunStyles, line_runs};
use super::{DocumentLayout, LineLayout, rows::rows};

/// Takes `style` explicitly since the editor also shapes outside any draw,
/// where `window.text_style()` doesn't apply. `wrap_width: None` never wraps.
pub(crate) fn shape_document(
    buffer: &Buffer,
    markdown: &Markdown,
    selection: &Range<ByteOffset>,
    marked: Option<&Range<ByteOffset>>,
    style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> DocumentLayout {
    let mut styles = RunStyles::new(style);
    let font_size = style.font_size.to_pixels(window.rem_size());

    let mut row_count = 0;
    // Spans and markers never cross a `\n`, so each line's are the next ones in order.
    let mut rest = markdown.spans.as_slice();
    let mut conceals = markdown.conceals.as_slice();
    let lines = buffer
        .lines()
        .map(|(start, text)| {
            let range = start..ByteOffset(start.0 + text.len());
            let own = rest
                .iter()
                .take_while(|span| span.range.start < range.end)
                .count();
            let (line_spans, next) = rest.split_at(own);
            rest = next;
            let own = conceals
                .iter()
                .take_while(|conceal| conceal.marker.start < range.end)
                .count();
            let (line_conceals, next) = conceals.split_at(own);
            conceals = next;
            let active = line_conceals
                .iter()
                .filter(|conceal| is_active(conceal, selection))
                .map(|conceal| (conceal.marker.clone(), conceal.replacement));
            let display = DisplayLine::new(range.clone(), active);
            let shown = SharedString::from(display.text(text).into_owned());
            let runs = line_runs(&range, line_spans, marked, &display, &mut styles);
            let wrapped = window
                .text_system()
                .shape_text(shown.clone(), font_size, &runs, wrap_width, None)
                .map_err(|err| eprintln!("failed to shape line at {start}: {err}"))
                .ok()
                .and_then(|mut shaped| shaped.pop())
                .unwrap_or_default();
            let rows = rows(&shown, &wrapped, &range, &display);
            let first_row = row_count;
            row_count += rows.len();
            LineLayout {
                wrapped,
                range,
                display,
                first_row,
                rows,
            }
        })
        .collect();

    DocumentLayout {
        lines,
        row_count,
        line_height: style.line_height_in_pixels(window.rem_size()),
        style: style.clone(),
        wrap_width,
    }
}
