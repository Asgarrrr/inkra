//! A quote's or task's gutter: its width and bars, measured once per text, and its revealed part
//! (markdown blocks spec, D3; task boxes spec, D3).

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::Gutter;
use inkra_text::ByteOffset;

use super::Prefix;

pub(in crate::editor::layout) struct GutterLayout {
    /// From the line's start, where a gutter starts.
    pub(super) len: usize,
    /// `Some` only while a part is revealed (a caret touches a `>` or the task part, spec D2).
    pub(super) shown: Option<Box<Shown>>,
    pub(super) geometry: Geometry,
    /// Whether the line's own task is checked, while its task part is concealed: a box paints.
    pub(super) boxed: Option<bool>,
}

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
    /// The checkbox's `x` span, measured as `width`; `None` with no task, and on a lazy line.
    pub(super) checkbox: Option<Range<Pixels>>,
}

/// What gutters share within one `shape_document`: the geometry of the line above, built or
/// carried, which a lazy line takes, and each distinct gutter text's geometry, measured once.
pub(in crate::editor::layout) struct GutterCarry<'a> {
    carried: Geometry,
    /// Geometry depends only on the gutter's text at the body size (spec D3); most are `> ` or `> > `.
    measured: HashMap<&'a str, Geometry>,
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

    /// `source` is the gutter's text. A gutter holding all its `>` measures
    /// itself; a lazy one takes the carried geometry. Only a revealed part keeps a shape, to paint it.
    pub(super) fn layout(
        &mut self,
        gutter: &Gutter,
        source: &'a str,
        reveal: Reveal,
        mut shape: impl FnMut(&Range<ByteOffset>, &str, Pixels) -> ShapedLine,
    ) -> GutterLayout {
        let body_size = self.body_size;
        let shaped = (reveal.quote || reveal.task).then(|| shape(&gutter.range, source, body_size));
        let geometry = if gutter.quotes.len() == gutter.depth {
            if let Some(geometry) = self.measured.get(source) {
                geometry.clone()
            } else {
                let measured = measured_text(source, gutter);
                let geometry = if let (Some(shaped), Cow::Borrowed(_)) = (&shaped, &measured) {
                    Geometry::measure(gutter, shaped, reveal.split)
                } else {
                    let shaped = shape(&gutter.range, &measured, body_size);
                    Geometry::measure(gutter, &shaped, reveal.split)
                };
                self.measured.insert(source, geometry.clone());
                geometry
            }
        } else {
            self.carried.clone()
        };
        GutterLayout {
            len: source.len(),
            shown: shaped.map(|shaped| Box::new(Shown::new(shaped, reveal))),
            geometry,
            boxed: gutter
                .task
                .as_ref()
                .filter(|_| !reveal.task)
                .map(|task| task.checked),
        }
    }
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
            checkbox: gutter.task.as_ref().map(|task| {
                shaped.x_for_index(task.checkbox.start - start)
                    ..shaped.x_for_index(task.checkbox.end - start)
            }),
        }
    }

    fn quote_part(&self) -> Self {
        Self {
            width: self.quote_width,
            bars: Rc::clone(&self.bars),
            quote_width: self.quote_width,
            checkbox: None,
        }
    }
}
