use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Bounds, Pixels, ShapedLine, SharedString, TextRun, Window, WrappedLine, point, px};
use inkra_markdown::{CodeBlock, Conceal, DisplayLine, Gutter, Heading, Span, is_active};
use inkra_text::ByteOffset;

use super::blocks::{Blocks, Labels};
use super::prefix::{GutterCarry, Hanging, Prefix, indent};
use super::runs::{RunStyles, line_runs};
use super::{DocumentLayout, LayoutInputs, LineLayout, rows::rows};
use crate::theme;

#[cfg(test)]
thread_local! {
    static SHAPES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static LINES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whole-document shapes so far on this thread; a keystroke's budget is one.
#[cfg(test)]
pub(crate) fn shape_count() -> usize {
    SHAPES.with(std::cell::Cell::get)
}

/// Lines handed to `shape_text` so far on this thread.
#[cfg(test)]
pub(crate) fn line_shape_count() -> usize {
    LINES.with(std::cell::Cell::get)
}

/// One line's `shape_text` output and every argument it took; a later layout reuses it only when all match.
pub(super) struct LineShape {
    pub(super) wrapped: WrappedLine,
    text: SharedString,
    font_size: Pixels,
    runs: Vec<TextRun>,
    wrap: Option<Pixels>,
}

impl LineShape {
    fn new(
        text: &str,
        font_size: Pixels,
        runs: Vec<TextRun>,
        wrap: Option<Pixels>,
        start: ByteOffset,
        window: &mut Window,
    ) -> Self {
        #[cfg(test)]
        LINES.with(|lines| lines.set(lines.get() + 1));
        let text = SharedString::from(text.to_owned());
        let wrapped = window
            .text_system()
            .shape_text(text.clone(), font_size, &runs, wrap, None)
            .map_err(|err| log::warn!("failed to shape line at {start}: {err}"))
            .ok()
            .and_then(|mut shaped| shaped.pop())
            .unwrap_or_default();
        Self {
            wrapped,
            text,
            font_size,
            runs,
            wrap,
        }
    }

    fn shaped_from(
        &self,
        text: &str,
        font_size: Pixels,
        runs: &[TextRun],
        wrap: Option<Pixels>,
    ) -> bool {
        self.font_size == font_size
            && self.wrap == wrap
            && self.text.as_ref() == text
            && self.runs == runs
    }
}

/// Takes `style` explicitly since the editor also shapes outside any draw,
/// where `window.text_style()` doesn't apply. `wrap_width: None` never wraps.
/// A line carries a shape from `previous` when every `shape_text` argument matches.
pub(crate) fn shape_document(
    inputs: &LayoutInputs<'_>,
    previous: Option<&DocumentLayout>,
    window: &mut Window,
) -> DocumentLayout {
    #[cfg(test)]
    SHAPES.with(|shapes| shapes.set(shapes.get() + 1));
    let &LayoutInputs {
        buffer,
        markdown,
        selection,
        marked,
        style,
        wrap_width,
        theme,
    } = inputs;
    let mut styles = RunStyles::new(style, theme);
    let body_size = style.font_size.to_pixels(window.rem_size());
    let body_height = style.line_height_in_pixels(window.rem_size());

    let mut row_count = 0;
    let mut top = px(0.);
    let mut bars = Vec::new();
    let mut blocks = Blocks::new(wrap_width);
    let mut labels = Labels::new(style, theme);
    // Spans, markers and gutters never cross a `\n`, so each line's are the next ones in order.
    let mut rest = markdown.spans.as_slice();
    let mut conceals = markdown.conceals.as_slice();
    let mut gutters = markdown.gutters.as_slice();
    let mut headings = markdown.headings.as_slice();
    let mut code_blocks = markdown.code_blocks.as_slice();
    let mut carry = GutterCarry::new(body_size);
    let lines = buffer
        .lines()
        .enumerate()
        .map(|(index, (start, text))| {
            let range = start..start + text.len();
            let heading = line_holder(&mut headings, &range, |heading| &heading.range);
            let block = line_holder(&mut code_blocks, &range, |block| &block.range);
            let opening = heading_opening(heading, &range);
            let (font_size, row_height) = line_metrics(heading, block, (body_size, body_height));
            let line_spans = line_items(&mut rest, |span| span.range.start <= range.end);
            let line_conceals =
                line_items(&mut conceals, |conceal| conceal.marker.start <= range.end);
            let line_gutters = line_items(&mut gutters, |gutter| gutter.range.start <= range.end);
            let gutter = line_gutters
                .first()
                .map(|gutter| (gutter, &text[..gutter.range.end - start]));
            let label = label_text(block, &range, text, line_conceals, selection)
                .map(|label| labels.shape(label, window));
            let (prefix, text_conceals) = line_prefix(
                gutter,
                &mut carry,
                opening,
                line_conceals,
                selection,
                font_size,
                |range, size| show(range, text, line_spans, marked, size, &mut styles, window),
            );
            let display = display_line(
                &range,
                text_conceals,
                prefix.as_ref().map(Prefix::range),
                selection,
            );
            let shown = display.text(text);
            let runs = line_runs(&range, line_spans, marked, &display, &mut styles);
            let wrap = wrap_width.map(|width| (width - indent(prefix.as_ref())).max(px(0.)));
            let near = previous
                .into_iter()
                .flat_map(|previous| candidates(previous, index, start, buffer.len()));
            let shape = carry_or_shape(near, &shown, font_size, runs, wrap, start, window);
            let rows = rows(&shown, &shape.wrapped, &range, &display);
            let first_row = row_count;
            row_count += rows.len();
            let line_top = top;
            top += rows_height(rows.len(), row_height);
            bars.extend(line_bars(prefix.as_ref(), line_top, top));
            let line = LineLayout {
                shape,
                range,
                display,
                first_row,
                rows,
                top: line_top,
                row_height,
                prefix,
            };
            blocks.line(block, label, &line, top);
            line
        })
        .collect();

    DocumentLayout {
        lines,
        row_count,
        height: top,
        bars,
        code_blocks: blocks.finish(),
        body_row_height: body_height,
        style: style.clone(),
        wrap_width,
        theme: Arc::clone(theme),
        version: buffer.version(),
        selection: selection.clone(),
        marked: marked.cloned(),
    }
}

/// The first candidate's shape that took exactly these `shape_text` arguments, else a new one.
fn carry_or_shape<'p>(
    candidates: impl Iterator<Item = &'p LineLayout>,
    text: &str,
    font_size: Pixels,
    runs: Vec<TextRun>,
    wrap: Option<Pixels>,
    start: ByteOffset,
    window: &mut Window,
) -> Rc<LineShape> {
    let mut shapes = candidates.map(|line| &line.shape);
    match shapes.find(|shape| shape.shaped_from(text, font_size, &runs, wrap)) {
        Some(shape) => Rc::clone(shape),
        None => Rc::new(LineShape::new(text, font_size, runs, wrap, start, window)),
    }
}

/// The marker that opens the line as a margin. Inside a container it keeps its inline conceal:
/// no margin exists there.
fn heading_opening(
    heading: Option<&Heading>,
    range: &Range<ByteOffset>,
) -> Option<Range<ByteOffset>> {
    heading
        .and_then(|heading| heading.marker.clone())
        .filter(|marker| marker.start == range.start)
}

/// A heading line takes its level's size and row height; a block line the code size in the body's
/// row; any other line the body's. A line never holds both.
fn line_metrics(
    heading: Option<&Heading>,
    block: Option<&CodeBlock>,
    body: (Pixels, Pixels),
) -> (Pixels, Pixels) {
    if let Some(heading) = heading {
        let metrics = theme::heading_metrics(heading.level);
        (metrics.size, metrics.line_height)
    } else if block.is_some() {
        (theme::CODE_BLOCK_TEXT_SIZE, body.1)
    } else {
        body
    }
}

/// The language a block's label shows: it has one, the block opens on this line, and its opening
/// fence is concealed. `text` is the line's own text.
fn label_text<'t>(
    block: Option<&CodeBlock>,
    line: &Range<ByteOffset>,
    text: &'t str,
    conceals: &[Conceal],
    selection: &Range<ByteOffset>,
) -> Option<&'t str> {
    let block = block.filter(|block| block.range.start >= line.start)?;
    let language = block.language.as_ref()?;
    conceals
        .iter()
        .any(|conceal| conceal.element == block.range && is_active(conceal, selection))
        .then(|| &text[language.start - line.start..language.end - line.start])
}

/// The previous layout's line at `index`, then the one the edit moved to `start`, as reuse candidates.
/// After one contiguous edit, the lines below it start `previous length - len` bytes from where they started.
fn candidates(
    previous: &DocumentLayout,
    index: usize,
    start: ByteOffset,
    len: ByteOffset,
) -> impl Iterator<Item = &LineLayout> {
    let moved = previous
        .lines
        .last()
        .and_then(|last| (start.get() + last.range.end.get()).checked_sub(len.get()))
        .and_then(|target| {
            previous
                .lines
                .binary_search_by_key(&ByteOffset::new(target), |line| line.range.start)
                .ok()
        });
    [Some(index), moved]
        .into_iter()
        .flatten()
        .filter_map(|index| previous.lines.get(index))
}

fn rows_height(rows: usize, row_height: Pixels) -> Pixels {
    #[allow(clippy::cast_precision_loss)]
    let rows = rows as f32;
    row_height * rows
}

/// The item of a sorted cursor that holds the line at `range`; `rest` moves past the items before
/// it. A setext heading spans lines, and so does a block: it stays first until a line starts past
/// its end.
fn line_holder<'a, T>(
    rest: &mut &'a [T],
    range: &Range<ByteOffset>,
    of: impl Fn(&T) -> &Range<ByteOffset>,
) -> Option<&'a T> {
    while rest.first().is_some_and(|item| of(item).end < range.start) {
        *rest = &rest[1..];
    }
    rest.first().filter(|item| of(item).start <= range.end)
}

/// One bar per quote level, from the line's top to the next line's, so a quote's bars meet.
fn line_bars(
    prefix: Option<&Prefix>,
    top: Pixels,
    bottom: Pixels,
) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
    prefix.map_or(&[][..], Prefix::bars).iter().map(move |&x| {
        Bounds::from_corners(point(x, top), point(x + theme::QUOTE_BAR_WIDTH, bottom))
    })
}

/// The items of a sorted cursor that start on this line; `rest` moves past them. The next line's
/// start at `range.end + 1` at the earliest, so `start <= range.end` holds for an empty item too.
fn line_items<'a, T>(rest: &mut &'a [T], on_line: impl Fn(&T) -> bool) -> &'a [T] {
    let own = rest.iter().take_while(|item| on_line(item)).count();
    let (line, next) = rest.split_at(own);
    *rest = next;
    line
}

/// The conceals starting in a prefix are its own, and one the caret touches reveals it; returns the
/// prefix and the conceals left for the text.
fn line_prefix<'a, 'c>(
    gutter: Option<(&Gutter, &'a str)>,
    carry: &mut GutterCarry<'a>,
    opening: Option<Range<ByteOffset>>,
    conceals: &'c [Conceal],
    selection: &Range<ByteOffset>,
    font_size: Pixels,
    mut shape: impl FnMut(&Range<ByteOffset>, Pixels) -> ShapedLine,
) -> (Option<Prefix>, &'c [Conceal]) {
    let own = gutter
        .map(|(gutter, _)| &gutter.range)
        .or(opening.as_ref())
        .map_or(0, |prefix| {
            conceals.partition_point(|conceal| conceal.marker.start < prefix.end)
        });
    let (own, rest) = conceals.split_at(own);
    // A gap conceal skips a line's leading `[ \t>]*` and a heading's own marker is the only conceal
    // before `opening.end`, so every conceal before the prefix's end is the prefix's own.
    debug_assert!(match gutter {
        Some((gutter, _)) => own.len() == gutter.quotes.len(),
        None =>
            own.len() <= 1
                && own
                    .iter()
                    .all(|conceal| Some(&conceal.marker) == opening.as_ref()),
    });
    let revealed = own.iter().any(|conceal| !is_active(conceal, selection));
    let prefix = match gutter {
        Some((gutter, source)) => Some(Prefix::Gutter(
            carry.layout(gutter, source, revealed, &mut shape),
        )),
        None => opening.map(|range| {
            Prefix::Margin(Hanging {
                shaped: revealed.then(|| shape(&range, font_size)),
                range,
            })
        }),
    };
    (prefix, rest)
}

/// The prefix never enters the shaped text, so no reveal can re-wrap its line.
fn display_line(
    range: &Range<ByteOffset>,
    conceals: &[Conceal],
    prefix: Option<&Range<ByteOffset>>,
    selection: &Range<ByteOffset>,
) -> DisplayLine {
    // A lazy line's gutter hides nothing.
    let hidden = prefix
        .filter(|prefix| !prefix.is_empty())
        .map(|prefix| (prefix.clone(), ""));
    let active = conceals
        .iter()
        .filter(|conceal| is_active(conceal, selection))
        .map(|conceal| (conceal.marker.clone(), conceal.replacement));
    DisplayLine::new(range.clone(), hidden.into_iter().chain(active))
}

/// Shaped through `line_runs` like the body, so its weight, dim and IME underline match.
/// `range` starts the line, so its text is a prefix of `line`.
fn show(
    range: &Range<ByteOffset>,
    line: &str,
    spans: &[Span],
    marked: Option<&Range<ByteOffset>>,
    font_size: Pixels,
    styles: &mut RunStyles<'_>,
    window: &mut Window,
) -> ShapedLine {
    let identity = DisplayLine::new(range.clone(), []);
    let runs = line_runs(range, spans, marked, &identity, styles);
    let text = SharedString::from(line[..range.end - range.start].to_owned());
    window
        .text_system()
        .shape_line(text, font_size, &runs, None)
}
