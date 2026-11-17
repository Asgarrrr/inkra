//! A quote's or task's gutter: its width and bars, measured once per text, its revealed part, and a
//! concealed task's box and lead (markdown blocks spec, D3; task boxes spec, D3, D10).

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter};
use inkra_text::ByteOffset;

use super::{Hidden, Prefix};

pub(in crate::editor::layout) struct GutterLayout {
    /// From the line's start, where a gutter starts.
    pub(super) len: usize,
    /// `Some` only while a part is revealed (a caret touches a `>` or the task part, spec D2).
    pub(super) shown: Option<Box<Shown>>,
    pub(super) geometry: Geometry,
    /// The line's own task, while its task part is concealed: its lead and box paint.
    pub(super) task_box: Option<Box<TaskBox>>,
}

/// A concealed task's box and the lead before it, from the line's own task, never carried (D10).
pub(in crate::editor::layout) struct TaskBox {
    /// The checkbox's `x` span, measured as the gutter's width.
    pub(in crate::editor::layout) x: Range<Pixels>,
    pub(in crate::editor::layout) checked: bool,
    pub(in crate::editor::layout) lead: Lead,
}

/// A lead through its own conceals at the body size, and its `x`, the quote part's width; `None`
/// when blank.
pub(in crate::editor::layout) type Lead = Option<(Rc<ShapedLine>, Pixels)>;

impl GutterLayout {
    /// A revealed part maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
    pub(super) fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.shown.as_ref().and_then(|shown| shown.index_for_x(x));
        index.unwrap_or(self.len)
    }
}

/// Which parts of a gutter a caret touches, and the byte, from the gutter's start, where its task
/// part starts: its length when it has none.
#[derive(Clone, Copy)]
pub(super) struct Reveal {
    pub(super) quote: bool,
    pub(super) task: bool,
    pub(super) split: usize,
}

/// The revealed part of a gutter's text at the body size, or the whole text when both parts are: its
/// bytes from `from`, painted at `x`, where they stand in the whole (task boxes spec, D3).
pub(super) struct Shown {
    pub(super) shaped: ShapedLine,
    from: usize,
    pub(super) x: Pixels,
}

impl Shown {
    /// One whole shape of the gutter's text, split at the task part when only one part shows.
    fn new(shaped: ShapedLine, reveal: Reveal) -> Self {
        let left = reveal.quote || reveal.split == 0;
        let right = reveal.task || reveal.split == shaped.len();
        if left && right {
            return Self {
                shaped,
                from: 0,
                x: px(0.),
            };
        }
        let (quote, task) = shaped.split_at(reveal.split);
        if left {
            Self {
                shaped: quote,
                from: 0,
                x: px(0.),
            }
        } else {
            Self {
                x: quote.width(),
                shaped: task,
                from: reveal.split,
            }
        }
    }

    pub(super) fn x_for_index(&self, index: usize) -> Option<Pixels> {
        let within = index.checked_sub(self.from)?;
        (within < self.shaped.len()).then(|| self.x + self.shaped.x_for_index(within))
    }

    /// Left of the column lands as on the first glyph, as before the split; left of the shown part,
    /// or at its end, `None`, as `x_for_index` reads it.
    fn index_for_x(&self, x: Pixels) -> Option<usize> {
        let within = self.shaped.closest_index_for_x(x - self.x);
        (self.x <= x.max(px(0.)) && within < self.shaped.len()).then_some(self.from + within)
    }
}

/// A gutter's text start and bars, and the quote part's width, carried to lazy lines (spec D3).
#[derive(Clone, Default)]
pub(super) struct Geometry {
    pub(super) width: Pixels,
    /// Each `>`'s `x`, outermost first.
    pub(super) bars: Rc<[Pixels]>,
    /// Where the task part starts; `width` with no task.
    quote_width: Pixels,
}

/// What a gutter's text measures to: its geometry, and its task's checkbox span and lead.
type Measured = (Geometry, Option<(Range<Pixels>, Lead)>);

/// What gutters share within one `shape_document`: the geometry of the line above, built or
/// carried, which a lazy line takes, and each distinct gutter text's geometry, measured once.
pub(in crate::editor::layout) struct GutterCarry<'a> {
    carried: Geometry,
    /// Geometry and the lead depend only on the gutter's text at the body size (spec D3; task boxes
    /// spec, D10); most are `> ` or `> > `.
    measured: HashMap<&'a str, Measured>,
    body_size: Pixels,
}

impl<'a> GutterCarry<'a> {
    pub(in crate::editor::layout) fn new(body_size: Pixels) -> Self {
        Self {
            carried: Geometry::default(),
            measured: HashMap::new(),
            body_size,
        }
    }

    /// Every line laid out passes here, in order. A line with no gutter passes none on, so a lazy
    /// line reads only the line above, which the walk builds or carries first (D7). It carries the
    /// quote part only: a task's marker is its own line's (task boxes spec, D3).
    pub(in crate::editor::layout) fn pass(&mut self, prefix: Option<&Prefix>) {
        self.carried = prefix
            .and_then(Prefix::geometry)
            .map(Geometry::quote_part)
            .unwrap_or_default();
    }

    /// `source` is the gutter's text and `lead` its lead's conceals. A gutter holding all its `>`
    /// measures itself; a lazy one takes the carried geometry. Only a revealed part keeps a shape,
    /// to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        reveal: Reveal,
        lead: &[Conceal],
        mut shape: impl FnMut(&Range<ByteOffset>, &[Hidden], &str, Pixels) -> ShapedLine,
    ) -> GutterLayout {
        let body_size = self.body_size;
        let shaped =
            (reveal.quote || reveal.task).then(|| shape(&gutter.range, &[], source, body_size));
        let (geometry, task) = if gutter.quotes.len() == gutter.depth {
            if let Some(measured) = self.measured.get(source) {
                measured.clone()
            } else {
                let measured = measure(
                    gutter,
                    source,
                    shaped.as_ref(),
                    reveal.split,
                    lead,
                    |range, hidden, text| shape(range, hidden, text, body_size),
                );
                self.measured.insert(source, measured.clone());
                measured
            }
        } else {
            (self.carried.clone(), None)
        };
        GutterLayout {
            len: source.len(),
            shown: shaped.map(|shaped| Box::new(Shown::new(shaped, reveal))),
            geometry,
            task_box: gutter.task.as_ref().zip(task).filter(|_| !reveal.task).map(
                |(task, (x, lead))| {
                    Box::new(TaskBox {
                        x,
                        checked: task.checked,
                        lead,
                    })
                },
            ),
        }
    }
}

/// The geometry of `source`, measured on `measured_text`, with `shaped` when that is `source`
/// itself; a task's checkbox span on the same shape, and its lead through `lead`, its conceals.
fn measure(
    gutter: &Gutter,
    source: &str,
    shaped: Option<&ShapedLine>,
    split: usize,
    lead: &[Conceal],
    mut shape: impl FnMut(&Range<ByteOffset>, &[Hidden], &str) -> ShapedLine,
) -> Measured {
    let measured = measured_text(source, gutter);
    let own;
    let whole = if let (Some(shaped), Cow::Borrowed(_)) = (shaped, &measured) {
        shaped
    } else {
        own = shape(&gutter.range, &[], &measured);
        &own
    };
    let geometry = Geometry::measure(gutter, whole, split);
    let start = gutter.range.start;
    let task = gutter.task.as_ref().map(|task| {
        let x = whole.x_for_index(task.checkbox.start - start)
            ..whole.x_for_index(task.checkbox.end - start);
        let text = &source[task.start - start..task.run - start];
        let blank = text.bytes().all(|byte| matches!(byte, b' ' | b'\t'));
        let lead = (!blank).then(|| {
            let hidden: Vec<Hidden> = lead
                .iter()
                .map(|conceal| {
                    let within = |offset| ByteOffset::new(offset - task.start);
                    (
                        within(conceal.marker.start)..within(conceal.marker.end),
                        conceal.replacement,
                    )
                })
                .collect();
            let shaped = shape(&(task.start..task.run), &hidden, text);
            (Rc::new(shaped), geometry.quote_width)
        });
        (x, lead)
    });
    (geometry, task)
}

/// What a gutter measures: its text with its box's middle byte an `X`, the widest state, so every
/// state paints inside one width and a toggle never changes it (task boxes spec, D3, C).
pub(super) fn measured_text<'s>(source: &'s str, gutter: &Gutter) -> Cow<'s, str> {
    match &gutter.task {
        Some(task) => {
            let middle = task.checkbox.start - gutter.range.start + 1;
            let mut text = source.to_owned();
            text.replace_range(middle..=middle, "X");
            Cow::Owned(text)
        }
        None => Cow::Borrowed(source),
    }
}

impl Geometry {
    fn measure(gutter: &Gutter, shaped: &ShapedLine, split: usize) -> Self {
        let start = gutter.range.start;
        Self {
            width: shaped.width(),
            bars: gutter
                .quotes
                .iter()
                .map(|quote| shaped.x_for_index(*quote - start))
                .collect(),
            quote_width: shaped.x_for_index(split),
        }
    }

    fn quote_part(&self) -> Self {
        Self {
            width: self.quote_width,
            bars: Rc::clone(&self.bars),
            quote_width: self.quote_width,
        }
    }
}
