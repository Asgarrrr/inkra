//! A quote's or task's gutter: its width and bars, measured once per text, its revealed part, and a
//! concealed task's box and lead (markdown blocks spec, D3; task boxes spec, D3, D10).

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine, px};
use inkra_markdown::{Conceal, Gutter};
use inkra_text::ByteOffset;

use super::{Hidden, Prefix};
use crate::theme::TASK_BOX_SIZE;

pub(in crate::editor::layout) struct GutterLayout {
    /// From the line's start, where a gutter starts.
    pub(super) len: usize,
    /// `Some` only while a part is revealed (a caret touches a `>` or the task part, spec D2).
    pub(super) shown: Option<Box<Shown>>,
    pub(super) geometry: Geometry,
    /// The line's own task, while its task part is concealed: its lead and box paint.
    pub(super) task_box: Option<Box<TaskBox>>,
}

/// A concealed task's shape and state, from the line's own task, never carried (D10).
pub(in crate::editor::layout) struct TaskBox {
    pub(in crate::editor::layout) shape: TaskShape,
    pub(in crate::editor::layout) checked: bool,
}

/// What a task measures to with its gutter's text, concealed, in every state (task boxes spec, D3).
#[derive(Clone)]
pub(in crate::editor::layout) struct TaskShape {
    /// The box's left: the quote part's width plus the lead's.
    pub(in crate::editor::layout) x: Pixels,
    /// The lead through its own conceals at the body size; `None` when blank.
    lead: Option<Rc<ShapedLine>>,
}

impl GutterLayout {
    /// A concealed task's lead, painted at the task part's `x`, the quote part's width.
    pub(super) fn lead(&self) -> Option<(&ShapedLine, Pixels)> {
        let lead = self.task_box.as_ref()?.shape.lead.as_deref()?;
        Some((lead, self.geometry.quote_width))
    }

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

/// A gutter's text start and bars, and the quote part's width, carried to lazy lines (spec D3). A
/// task line's text starts past its box slot while concealed, past its raw text while revealed.
#[derive(Clone, Default)]
pub(super) struct Geometry {
    pub(super) width: Pixels,
    /// Each `>`'s `x`, outermost first.
    pub(super) bars: Rc<[Pixels]>,
    /// Where the task part starts; `width` with no task.
    quote_width: Pixels,
}

/// What a gutter's text measures to concealed, once per text.
#[derive(Clone)]
struct Measured {
    geometry: Geometry,
    /// `None` with no task.
    task: Option<TaskShape>,
}

/// What gutters share within one `shape_document`: the geometry of the line above, built or
/// carried, which a lazy line takes, and each distinct gutter text's geometry, measured once.
pub(in crate::editor::layout) struct GutterCarry<'a> {
    carried: Geometry,
    /// Geometry and the lead depend only on the gutter's text at the body size (spec D3; task boxes
    /// spec, D10); most are `> ` or `> > `.
    measured: HashMap<&'a str, Measured>,
    body_size: Pixels,
    /// The box slot, measured at the first task.
    slot: Option<Pixels>,
}

impl<'a> GutterCarry<'a> {
    pub(in crate::editor::layout) fn new(body_size: Pixels) -> Self {
        Self {
            carried: Geometry::default(),
            measured: HashMap::new(),
            body_size,
            slot: None,
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
    /// to paint it, and a revealed task part widens the gutter to that shape.
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
        let measured = if gutter.quotes.len() == gutter.depth {
            if let Some(measured) = self.measured.get(source) {
                measured.clone()
            } else {
                let measured = measure(
                    gutter,
                    source,
                    lead,
                    &mut self.slot,
                    |range, hidden, text| shape(range, hidden, text, body_size),
                );
                self.measured.insert(source, measured.clone());
                measured
            }
        } else {
            Measured {
                geometry: self.carried.clone(),
                task: None,
            }
        };
        let Measured { mut geometry, task } = measured;
        let raw = shaped.as_ref().filter(|_| reveal.task);
        geometry.width = raw.map_or(geometry.width, ShapedLine::width);
        GutterLayout {
            len: source.len(),
            shown: shaped.map(|shaped| Box::new(Shown::new(shaped, reveal))),
            geometry,
            task_box: gutter.task.as_ref().zip(task).filter(|_| !reveal.task).map(
                |(task, shape)| {
                    Box::new(TaskBox {
                        shape,
                        checked: task.checked,
                    })
                },
            ),
        }
    }
}

/// The concealed geometry of `source`: its quote part on its own text, then a task's lead through
/// `lead`, its conceals, and the box slot, which `slot` holds once measured.
fn measure(
    gutter: &Gutter,
    source: &str,
    lead: &[Conceal],
    slot: &mut Option<Pixels>,
    mut shape: impl FnMut(&Range<ByteOffset>, &[Hidden], &str) -> ShapedLine,
) -> Measured {
    let start = gutter.range.start;
    let split = gutter.quote_end() - start;
    let quote =
        (!gutter.quotes.is_empty()).then(|| shape(&(start..start + split), &[], &source[..split]));
    let mut geometry = Geometry::measure(gutter, quote.as_ref());
    let task = gutter.task.as_ref().map(|task| {
        let text = &source[task.start - start..task.run - start];
        let shaped = (!text.is_empty()).then(|| {
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
            shape(&(task.start..task.run), &hidden, text)
        });
        let x = geometry.quote_width + shaped.as_ref().map_or(px(0.), ShapedLine::width);
        let slot = *slot.get_or_insert_with(|| {
            // The box run carries only styles that keep a glyph's advance, so any task's measures
            // the slot for all.
            let run = task.run..gutter.range.end;
            let hidden = [(
                ByteOffset::default()..ByteOffset::new(run.end - run.start),
                "• ",
            )];
            let bullet = shape(&run, &hidden, &source[task.run - start..]);
            bullet.width().max(TASK_BOX_SIZE)
        });
        geometry.width = x + slot;
        let blank = text.bytes().all(|byte| matches!(byte, b' ' | b'\t'));
        TaskShape {
            x,
            lead: shaped.filter(|_| !blank).map(Rc::new),
        }
    });
    Measured { geometry, task }
}

impl Geometry {
    /// From the quote part's own shape; `None` with no `>`.
    fn measure(gutter: &Gutter, quote: Option<&ShapedLine>) -> Self {
        let start = gutter.range.start;
        let x = |index| quote.map_or(px(0.), |shaped| shaped.x_for_index(index));
        let quote_width = quote.map_or(px(0.), ShapedLine::width);
        Self {
            width: quote_width,
            bars: gutter
                .quotes
                .iter()
                .map(|quote| x(*quote - start))
                .collect(),
            quote_width,
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
