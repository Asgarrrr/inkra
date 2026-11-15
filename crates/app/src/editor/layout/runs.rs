use std::collections::HashMap;
use std::ops::Range;

use gpui::{HighlightStyle, TextRun, TextStyle, UnderlineStyle, px};
use inkra_markdown::{DisplayLine, Span, Styles};
use inkra_text::ByteOffset;

use crate::decorations::Drawn;
use crate::theme::{self, ColorToken, Theme};

/// Resolves each `(Styles, composing, colour)` once per `shape_document` call: that
/// runs every frame, and a `TextStyle` clone per run would be per-frame waste.
pub(super) struct RunStyles<'a> {
    base: &'a TextStyle,
    theme: &'a Theme,
    decorations: Drawn<'a>,
    resolved: HashMap<(Styles, bool, Option<ColorToken>), TextStyle>,
}

impl<'a> RunStyles<'a> {
    pub(super) fn new(base: &'a TextStyle, theme: &'a Theme, decorations: &Drawn<'a>) -> Self {
        Self {
            base,
            theme,
            decorations: *decorations,
            resolved: HashMap::new(),
        }
    }

    fn run(
        &mut self,
        styles: Styles,
        composing: bool,
        color: Option<ColorToken>,
        len: usize,
    ) -> TextRun {
        // Revealed syntax and marked text win over every layer (decoration layer spec, D7).
        let color = color.filter(|_| !composing && !styles.contains(Styles::MARKER));
        let (base, theme) = (self.base, self.theme);
        self.resolved
            .entry((styles, composing, color))
            .or_insert_with(|| {
                let mut style = theme::markdown_style(base, styles, theme);
                if let Some(token) = color {
                    style.color = theme.colors.color(token).into();
                }
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

/// Runs indexed from zero within the line's shown text; `display` is over the line's own offsets.
/// Every range is clamped to the line, so a stale span mis-colours one frame instead of breaking
/// the run lengths.
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
        cuts.extend([
            line.start + hidden.start.get(),
            line.start + hidden.end.get(),
        ]);
    }
    let decorations = styles.decorations.colors_in(line.clone());
    for (range, _) in &decorations {
        cuts.extend([range.start, range.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();

    // Each segment's colour, by its start's index in `cuts`; a later rank paints over. It stops at
    // the last coloured segment, so an undecorated line allocates none.
    let mut colors = Vec::new();
    for (range, token) in decorations {
        let first = cuts.partition_point(|&cut| cut < range.start);
        let last = cuts.partition_point(|&cut| cut < range.end);
        colors.resize(colors.len().max(last), None);
        colors[first..last].fill(Some(token));
    }

    let mut spans = spans.iter().peekable();
    let mut runs = Vec::with_capacity(cuts.len());
    for (ix, pair) in cuts.windows(2).enumerate() {
        let color = colors.get(ix).copied().flatten();
        let (start, end) = (pair[0], pair[1]);
        let shown = |offset: ByteOffset| display.to_display(ByteOffset::new(offset - line.start));
        let len = shown(end).get() - shown(start).get();
        if len == 0 {
            continue;
        }
        while spans.next_if(|span| span.range.end <= start).is_some() {}
        let span_styles = spans
            .peek()
            .filter(|span| span.range.start <= start)
            .map_or(Styles::NONE, |span| span.styles);
        let composing = marked.is_some_and(|marked| marked.start <= start && start < marked.end);
        runs.push(styles.run(span_styles, composing, color, len));
    }
    if runs.is_empty() {
        runs.push(styles.run(Styles::NONE, false, None, 0));
    }
    runs
}

#[cfg(test)]
mod tests;
