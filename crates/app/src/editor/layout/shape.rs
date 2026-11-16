use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Bounds, Pixels, ShapedLine, SharedString, TextRun, Window, point, px};
use inkra_markdown::{CodeBlock, Conceal, DisplayLine, Gutter, Heading, Markdown, Span, is_active};
use inkra_text::{Buffer, ByteOffset};

use super::blocks::{Blocks, Labels};
use super::dirty::carry;
use super::line::{LineLayout, LineShape, Placed};
use super::prefix::{GutterCarry, Prefix, heading_opening, indent, line_prefix};
use super::runs::{RunStyles, line_runs};
use super::{DocumentLayout, LayoutInputs, rows::rows};
use crate::theme;

#[cfg(test)]
thread_local! {
    static SHAPES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whole-document shapes so far on this thread; a keystroke's budget is one.
#[cfg(test)]
pub(crate) fn shape_count() -> usize {
    SHAPES.with(std::cell::Cell::get)
}

/// Lines built rather than carried so far on this thread.
#[cfg(test)]
pub(crate) fn line_build_count() -> usize {
    BUILDS.with(std::cell::Cell::get)
}

/// Takes `style` explicitly since the editor also shapes outside any draw,
/// where `window.text_style()` doesn't apply. `wrap_width: None` never wraps.
/// A clean line carries its whole layout from `previous`; a built one still carries a shape when
/// every `shape_text` argument matches.
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
        code_block_wrap,
        theme,
        scrolls,
        ..
    } = inputs;
    let body_size = style.font_size.to_pixels(window.rem_size());
    let body_height = style.line_height_in_pixels(window.rem_size());
    let text = buffer.text();
    let mut build = Build {
        inputs,
        previous,
        text,
        styles: RunStyles::new(style, theme, &inputs.decorations),
        labels: Labels::new(style, theme),
        gutter_carry: GutterCarry::new(body_size),
        body: (body_size, body_height),
        spans: &markdown.spans,
        conceals: &markdown.conceals,
        gutters: &markdown.gutters,
        headings: &markdown.headings,
    };
    let mut carry = previous.and_then(|previous| carry(inputs, previous));

    let mut lines = Vec::with_capacity(previous.map_or(0, |previous| previous.lines.len()));
    let mut row_count = 0;
    let mut top = px(0.);
    let mut bars = Vec::new();
    let mut blocks = Blocks::new(wrap_width, markdown.code_blocks.len());
    let mut code_blocks = markdown.code_blocks.as_slice();
    let mut at = 0;
    let mut above_built = false;
    loop {
        let start = ByteOffset::new(at);
        // A lazy quote line reads the geometry of the line above, so it follows its rebuild (D7).
        let lazy = above_built && build.lazy_gutter(&(start..start + line_len(text, at)));
        let clean = carry
            .as_mut()
            .filter(|_| !lazy)
            .and_then(|carry| carry.clean(start));
        above_built = clean.is_none();
        let len = clean.map_or_else(|| line_len(text, at), |line| line.len);
        // A carried length ends where the line does, or every line after it misplaces (spec, Risks).
        debug_assert!(at + len == text.len() || text.as_bytes().get(at + len) == Some(&b'\n'));
        let range = start..start + len;
        let block = line_holder(&mut code_blocks, &range, |block| &block.range);
        let line = match clean {
            Some(line) => Rc::clone(line),
            None => Rc::new(build.line(&range, block, lines.len(), window)),
        };
        build.gutter_carry.pass(line.prefix.as_ref());
        let placed = Placed {
            start,
            top,
            first_row: row_count,
            block: blocks.index_of(block),
            line,
        };
        row_count += placed.line.rows.len();
        top += rows_height(placed.line.rows.len(), placed.line.row_height);
        bars.extend(line_bars(placed.line.prefix.as_ref(), placed.top, top));
        blocks.line(block, &placed, top);
        lines.push(placed);
        at += len;
        if at == text.len() {
            break;
        }
        at += 1;
    }

    DocumentLayout {
        lines,
        row_count,
        height: top,
        bars,
        code_blocks: blocks.finish(),
        body_row_height: body_height,
        style: style.clone(),
        wrap_width,
        margin: inputs.margin,
        code_block_wrap,
        theme: Arc::clone(theme),
        shaped_at: (buffer.version(), inputs.decorations.revision()),
        buffer: buffer.id(),
        at: buffer.edit_mark(),
        markdown: Rc::clone(markdown),
        selection: selection.clone(),
        marked: marked.cloned(),
        scrolls: scrolls.clone(),
    }
}

/// The bytes of the line at `at`, without its `\n`.
fn line_len(text: &str, at: usize) -> usize {
    text[at..].find('\n').unwrap_or(text.len() - at)
}

/// What building a line reads, and the cursors over the parse; spans, markers and gutters never
/// cross a `\n`, so each line's are the next ones in order.
struct Build<'a, 'i> {
    inputs: &'i LayoutInputs<'a>,
    previous: Option<&'i DocumentLayout>,
    text: &'a str,
    styles: RunStyles<'a>,
    labels: Labels<'a>,
    gutter_carry: GutterCarry<'a>,
    /// The body's font size and row height.
    body: (Pixels, Pixels),
    spans: &'a [Span],
    conceals: &'a [Conceal],
    gutters: &'a [Gutter],
    headings: &'a [Heading],
}

impl Build<'_, '_> {
    /// Whether the line at `range` has a gutter missing some of its `>`.
    fn lazy_gutter(&self, range: &Range<ByteOffset>) -> bool {
        let next = self
            .gutters
            .partition_point(|gutter| gutter.range.start < range.start);
        self.gutters.get(next).is_some_and(|gutter| {
            gutter.range.start <= range.end && gutter.quotes.len() < gutter.depth
        })
    }

    /// The line at `range`, the `index`th, which `block` holds.
    fn line(
        &mut self,
        range: &Range<ByteOffset>,
        block: Option<&CodeBlock>,
        index: usize,
        window: &mut Window,
    ) -> LineLayout {
        #[cfg(test)]
        BUILDS.with(|builds| builds.set(builds.get() + 1));
        let &LayoutInputs {
            buffer,
            selection,
            marked,
            wrap_width,
            code_block_wrap,
            margin,
            ..
        } = self.inputs;
        let text = &self.text[range.start.get()..range.end.get()];
        let heading = line_holder(&mut self.headings, range, |heading| &heading.range);
        let (font_size, row_height) = line_metrics(heading, block, self.body);
        let line_spans = line_items(&mut self.spans, range, |span| span.range.start);
        let line_conceals = line_items(&mut self.conceals, range, |conceal| conceal.marker.start);
        let line_gutters = line_items(&mut self.gutters, range, |gutter| gutter.range.start);
        let gutter = line_gutters
            .first()
            .map(|gutter| (gutter, &text[..gutter.range.end - range.start]));
        let label = label_text(block, range, text, line_conceals, selection)
            .map(|label| self.labels.shape(label, window));
        let styles = &mut self.styles;
        let (prefix, text_conceals) = line_prefix(
            gutter,
            &mut self.gutter_carry,
            heading_opening(heading, range, text, font_size, margin),
            line_conceals,
            selection,
            |range, source, size| show(range, source, line_spans, marked, size, styles, window),
        );
        let display = display_line(
            range,
            text_conceals,
            prefix.as_ref().map(Prefix::range),
            selection,
        );
        let shown = display.text(text);
        let runs = line_runs(range, line_spans, marked, &display, &mut self.styles);
        let wrap = line_wrap(wrap_width, block, code_block_wrap, prefix.as_ref());
        let near = candidates(self.previous, index, range.start, buffer.len());
        let shape = carry_or_shape(near, &shown, font_size, runs, wrap, range.start, window);
        let len = range.end - range.start;
        let rows = rows(&shown, &shape.wrapped, len, &display);
        LineLayout {
            shape,
            len,
            display,
            rows,
            row_height,
            prefix,
            label,
        }
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

/// Whether a revealed heading marker opens its line, so `margin` decides whether it hangs (D13).
/// Only touched conceals reach the heading lookup, and a selection touches few.
pub(super) fn reads_margin(
    buffer: &Buffer,
    markdown: &Markdown,
    selection: &Range<ByteOffset>,
) -> bool {
    let text = buffer.text().as_bytes();
    markdown
        .conceals
        .iter()
        .filter(|conceal| !is_active(conceal, selection))
        .filter(|conceal| {
            let start = conceal.marker.start.get();
            start == 0 || text[start - 1] == b'\n'
        })
        .any(|conceal| {
            markdown
                .headings
                .iter()
                .any(|heading| heading.marker.as_ref() == Some(&conceal.marker))
        })
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
    previous: Option<&DocumentLayout>,
    index: usize,
    start: ByteOffset,
    len: ByteOffset,
) -> impl Iterator<Item = &LineLayout> {
    previous.into_iter().flat_map(move |previous| {
        let moved = previous
            .lines
            .last()
            .and_then(|last| (start.get() + last.end().get()).checked_sub(len.get()))
            .and_then(|target| {
                previous
                    .lines
                    .binary_search_by_key(&ByteOffset::new(target), |line| line.start)
                    .ok()
            });
        [Some(index), moved]
            .into_iter()
            .flatten()
            .filter_map(|index| previous.lines.get(index))
            .map(|placed| &*placed.line)
    })
}

/// A block line never wraps unless `code_block_wrap` (code block scroll spec, D2); any other wraps at the column less its indent.
fn line_wrap(
    wrap_width: Option<Pixels>,
    block: Option<&CodeBlock>,
    code_block_wrap: bool,
    prefix: Option<&Prefix>,
) -> Option<Pixels> {
    wrap_width
        .filter(|_| block.is_none() || code_block_wrap)
        .map(|width| (width - indent(prefix)).max(px(0.)))
}

fn rows_height(rows: usize, row_height: Pixels) -> Pixels {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count far below 2^24 converts to f32 exactly"
    )]
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

/// The items of a sorted cursor that start on the line at `range`; `rest` moves past them. The next
/// line starts at `range.end + 1` at the earliest, so `start <= range.end` holds for an empty item too.
fn line_items<'a, T>(
    rest: &mut &'a [T],
    range: &Range<ByteOffset>,
    start: impl Fn(&T) -> ByteOffset,
) -> &'a [T] {
    let first = rest.partition_point(|item| start(item) < range.start);
    let own = rest[first..].partition_point(|item| start(item) <= range.end);
    let (line, next) = rest[first..].split_at(own);
    *rest = next;
    line
}

/// Over the line's own offsets. The prefix never enters the shaped text, so no reveal can re-wrap
/// its line.
fn display_line(
    range: &Range<ByteOffset>,
    conceals: &[Conceal],
    prefix: Option<Range<usize>>,
    selection: &Range<ByteOffset>,
) -> DisplayLine {
    let within = |offset: ByteOffset| ByteOffset::new(offset - range.start);
    // A lazy line's gutter hides nothing.
    let hidden = prefix.filter(|prefix| !prefix.is_empty()).map(|prefix| {
        (
            ByteOffset::new(prefix.start)..ByteOffset::new(prefix.end),
            "",
        )
    });
    let active = conceals
        .iter()
        .filter(|conceal| is_active(conceal, selection))
        .map(|conceal| {
            (
                within(conceal.marker.start)..within(conceal.marker.end),
                conceal.replacement,
            )
        });
    DisplayLine::new(
        within(range.start)..within(range.end),
        hidden.into_iter().chain(active),
    )
}

/// Shaped through `line_runs` like the body, so its weight, dim and IME underline match.
/// `source` is what `range` shows: its own bytes, or as many others.
fn show(
    range: &Range<ByteOffset>,
    source: &str,
    spans: &[Span],
    marked: Option<&Range<ByteOffset>>,
    font_size: Pixels,
    styles: &mut RunStyles<'_>,
    window: &mut Window,
) -> ShapedLine {
    let len = range.end - range.start;
    let identity = DisplayLine::new(ByteOffset::default()..ByteOffset::new(len), []);
    let runs = line_runs(range, spans, marked, &identity, styles);
    let text = SharedString::from(source.to_owned());
    window
        .text_system()
        .shape_line(text, font_size, &runs, None)
}
