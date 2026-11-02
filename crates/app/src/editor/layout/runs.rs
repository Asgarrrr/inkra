use std::collections::HashMap;
use std::ops::Range;

use gpui::{HighlightStyle, TextRun, TextStyle, UnderlineStyle, px};
use inkra_markdown::{DisplayLine, Span, Styles};
use inkra_text::ByteOffset;

use crate::theme;

/// Resolves each `(Styles, composing)` once per `shape_document` call: that
/// runs every frame, and a `TextStyle` clone per run would be per-frame waste.
pub(super) struct RunStyles<'a> {
    base: &'a TextStyle,
    resolved: HashMap<(Styles, bool), TextStyle>,
}

impl<'a> RunStyles<'a> {
    pub(super) fn new(base: &'a TextStyle) -> Self {
        Self {
            base,
            resolved: HashMap::new(),
        }
    }

    fn run(&mut self, styles: Styles, composing: bool, len: usize) -> TextRun {
        let base = self.base;
        self.resolved
            .entry((styles, composing))
            .or_insert_with(|| {
                let style = theme::markdown_style(base, styles);
                if composing { underlined(style) } else { style }
            })
            .to_run(len)
    }
}

fn underlined(style: TextStyle) -> TextStyle {
    let color = style.color;
    style.highlight(HighlightStyle {
        underline: Some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        }),
        ..HighlightStyle::default()
    })
}

/// Runs indexed from zero within the line's shown text. Every range is clamped to the line,
/// so a stale span mis-colours one frame instead of breaking the run lengths.
pub(super) fn line_runs(
    line: &Range<ByteOffset>,
    spans: &[Span],
    marked: Option<&Range<ByteOffset>>,
    display: &DisplayLine,
    styles: &mut RunStyles<'_>,
) -> Vec<TextRun> {
    let clamp = |offset: ByteOffset| offset.clamp(line.start, line.end);

    let mut cuts = vec![line.start, line.end];
    for span in spans {
        cuts.extend([clamp(span.range.start), clamp(span.range.end)]);
    }
    if let Some(marked) = marked {
        cuts.extend([clamp(marked.start), clamp(marked.end)]);
    }
    for hidden in display.hidden() {
        cuts.extend([hidden.start, hidden.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut spans = spans.iter().peekable();
    let mut runs = Vec::with_capacity(cuts.len());
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let len = display.to_display(end).get() - display.to_display(start).get();
        if len == 0 {
            continue;
        }
        while spans.next_if(|span| span.range.end <= start).is_some() {}
        let span_styles = spans
            .peek()
            .filter(|span| span.range.start <= start)
            .map_or(Styles::NONE, |span| span.styles);
        let composing = marked.is_some_and(|marked| marked.start <= start && start < marked.end);
        runs.push(styles.run(span_styles, composing, len));
    }
    if runs.is_empty() {
        runs.push(styles.run(Styles::NONE, false, 0));
    }
    runs
}

#[cfg(test)]
mod tests;
