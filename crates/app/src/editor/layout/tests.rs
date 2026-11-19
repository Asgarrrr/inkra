//! The oracle: a layout carried through any steps matches a fresh one (incremental layout spec, D10).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext, Entity, Pixels, TestAppContext, Window, px};
use inkra_text::{Bias, Buffer, ByteOffset, Diff};
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

use super::line::Placed;
use super::{BlockScrolls, DocumentLayout, LayoutInputs, line_build_count, shape_document};
use crate::decorations;
use crate::theme::{Appearance, Theme, builtin};

mod layer;
mod list_width;
mod toggle;

const TOKENS: &[&str] = &[
    "a", "word ", " ", "\n", "\n", "\n\n", "```", "```rust", "`", "> ", "> > ", ">", "  > ", "# ",
    "## ", "===", "---", "*", "**", "_", "[[", "]]", "[l](u)", "#h", "- [ ] ", "- [x] ", "- ",
    "1. ", "9. ", "10. ", "2) ", "* ", "    ", "  ", "\t", "é", "日", "\n  ", "- a\nb",
];

fn markdown(max: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(TOKENS), 0..max).prop_map(|tokens| tokens.concat())
}

/// A splice in the text it lands on: a start, whether it moves to its line's start, where block
/// markers change, a removed length past it, the inserted text.
type Edit = (Index, bool, usize, String);

fn edit() -> impl Strategy<Value = Edit> {
    (any::<Index>(), any::<bool>(), 0..8usize, markdown(4))
}

#[derive(Debug, Clone)]
enum Step {
    Splice(Edit),
    /// A splice laid out before `decorations::sync` brings the layer to it.
    Unsynced(Edit),
    /// Several hunks reach the buffer as one `apply_diff`, as a reload does.
    Diff(Vec<Edit>),
    /// Several splices in one transaction, so its undo or redo is a batch.
    Group(Vec<Edit>),
    Undo,
    Redo,
    /// A composition: the inserted text stays marked.
    Compose(Edit),
    /// `unmark_text`, or a restore's `reset_state`: both drop the composition.
    Unmark,
    /// A selection between two points, in either order; a caret when they meet.
    Select(Index, Index),
    Caret(Index),
    /// The selection's end moves to a point and its start stays, as a drag does.
    Drag(Index),
    /// A click on a concealed task's box, picked among the carried layout's.
    Toggle(Index),
    /// `⌘L` on a selection between two points: its batch as one `apply_diff`.
    Cycle(Index, Index),
    Wrap(Option<f32>),
    Margin,
    CodeBlockWrap,
    Theme,
    Paint(layer::Paint),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => edit().prop_map(Step::Splice),
        1 => edit().prop_map(Step::Unsynced),
        4 => layer::paint().prop_map(Step::Paint),
        2 => prop::collection::vec(edit(), 2..5).prop_map(Step::Diff),
        2 => prop::collection::vec(edit(), 2..5).prop_map(Step::Group),
        2 => Just(Step::Undo),
        1 => Just(Step::Redo),
        2 => edit().prop_map(Step::Compose),
        1 => Just(Step::Unmark),
        3 => (any::<Index>(), any::<Index>()).prop_map(|(a, b)| Step::Select(a, b)),
        2 => any::<Index>().prop_map(Step::Caret),
        2 => any::<Index>().prop_map(Step::Drag),
        3 => any::<Index>().prop_map(Step::Toggle),
        3 => (any::<Index>(), any::<Index>()).prop_map(|(a, b)| Step::Cycle(a, b)),
        1 => prop::sample::select(vec![None, Some(120.), Some(300.)]).prop_map(Step::Wrap),
        1 => Just(Step::Margin),
        1 => Just(Step::CodeBlockWrap),
        1 => Just(Step::Theme),
    ]
}

/// `offset`, clamped to `text` and moved back to a char boundary.
fn floor(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// A char boundary of `buffer`'s text, picked by `at`.
fn point(buffer: &Buffer, at: Index) -> ByteOffset {
    let text = buffer.text();
    ByteOffset::new(floor(text, at.index(text.len() + 1)))
}

/// The edit as a byte range and its text, on char boundaries of `text`.
fn resolve(
    text: &str,
    (start, line_start, removed, inserted): &Edit,
) -> (Range<ByteOffset>, String) {
    let floor = |offset| floor(text, offset);
    let mut start = floor(start.index(text.len() + 1));
    if *line_start {
        start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    }
    let end = floor(start + removed).max(start);
    (
        ByteOffset::new(start)..ByteOffset::new(end),
        inserted.clone(),
    )
}

/// Each edit resolved against the text the ones before it leave, and that final text.
fn resolve_all(text: &str, edits: &[Edit]) -> (Vec<(Range<ByteOffset>, String)>, String) {
    let mut text = text.to_owned();
    let resolved = edits
        .iter()
        .map(|edit| {
            let (range, inserted) = resolve(&text, edit);
            text.replace_range(range.start.get()..range.end.get(), &inserted);
            (range, inserted)
        })
        .collect();
    (resolved, text)
}

/// Every layout input a step can change, beside the buffer.
struct Inputs {
    selection: Range<ByteOffset>,
    marked: Option<Range<ByteOffset>>,
    wrap_width: Option<Pixels>,
    margin: Pixels,
    code_block_wrap: bool,
    theme: &'static Arc<Theme>,
}

impl Inputs {
    /// `layout` is the carried layout, the one a click lands on.
    fn apply(&mut self, buffer: &mut Buffer, step: &Step, layout: &DocumentLayout) {
        match step {
            Step::Splice(edit) | Step::Unsynced(edit) => {
                let (range, inserted) = resolve(buffer.text(), edit);
                buffer.transact(|txn| txn.replace(range, &inserted));
                self.marked = None;
            }
            Step::Diff(edits) => {
                let (_, after) = resolve_all(buffer.text(), edits);
                let diff = inkra_text::diff(buffer.version(), buffer.text(), &after);
                assert!(buffer.apply_diff(&diff));
                self.marked = None;
            }
            Step::Group(edits) => {
                let (resolved, _) = resolve_all(buffer.text(), edits);
                buffer.transact(|txn| {
                    for (range, inserted) in resolved {
                        txn.replace(range, &inserted);
                    }
                });
                self.marked = None;
            }
            Step::Undo => {
                buffer.undo();
                self.marked = None;
            }
            Step::Redo => {
                buffer.redo();
                self.marked = None;
            }
            Step::Compose(edit) => {
                let (range, inserted) = resolve(buffer.text(), edit);
                let start = range.start;
                buffer.transact(|txn| txn.replace(range, &inserted));
                self.marked = (!inserted.is_empty()).then_some(start..start + inserted.len());
            }
            Step::Unmark => self.marked = None,
            Step::Select(a, b) => {
                let [a, b] = [*a, *b].map(|at| point(buffer, at));
                self.selection = a.min(b)..a.max(b);
            }
            Step::Caret(at) => {
                let at = point(buffer, *at);
                self.selection = at..at;
            }
            Step::Drag(head) => {
                let (anchor, head) = (self.selection.start, point(buffer, *head));
                self.selection = anchor.min(head)..anchor.max(head);
            }
            Step::Toggle(pick) => {
                let boxes: Vec<_> = layout
                    .lines
                    .iter()
                    .filter_map(|placed| {
                        let (square, _) = placed.line.task_box()?;
                        Some(square.center() + gpui::point(px(0.), placed.top))
                    })
                    .collect();
                if !boxes.is_empty() {
                    let click = boxes[pick.index(boxes.len())];
                    let (flip, text_start) = layout.task_at(click).expect("a box takes its click");
                    assert!(buffer.apply_diff(&flip));
                    let text_start = flip.map(text_start, Bias::Right);
                    self.selection = text_start..text_start;
                }
                self.marked = None;
            }
            Step::Cycle(a, b) => {
                let [a, b] = [*a, *b].map(|at| point(buffer, at));
                let (start, end) = (a.min(b), a.max(b));
                let markdown = inkra_markdown::parse(buffer.text());
                let hunks = inkra_markdown::cycle_tasks(buffer.text(), &markdown, start..end);
                let cycle = Diff::new(buffer.version(), hunks);
                assert!(buffer.apply_diff(&cycle));
                let start = cycle.map(start, Bias::Right);
                self.selection = start..cycle.map(end, Bias::Left).max(start);
                self.marked = None;
            }
            Step::Wrap(width) => self.wrap_width = width.map(px),
            Step::Margin => {
                self.margin = if self.margin == px(76.) {
                    px(20.)
                } else {
                    px(76.)
                };
            }
            Step::CodeBlockWrap => self.code_block_wrap = !self.code_block_wrap,
            Step::Theme => {
                let light = builtin::of(Appearance::Light);
                self.theme = if Arc::ptr_eq(self.theme, light) {
                    builtin::dark()
                } else {
                    light
                };
            }
            Step::Paint(_) => {}
        }
        let text = buffer.text();
        let clamp = |at: ByteOffset| ByteOffset::new(floor(text, at.get()));
        self.selection = clamp(self.selection.start)..clamp(self.selection.end);
    }

    fn layout(
        &self,
        buffer: &Entity<Buffer>,
        previous: Option<&DocumentLayout>,
        window: &mut Window,
        cx: &App,
    ) -> DocumentLayout {
        let decorations = decorations::drawn(buffer, cx);
        let buffer = buffer.read(cx);
        let markdown = Rc::new(inkra_markdown::parse(buffer.text()));
        let mut style = window.text_style();
        // Off `CODE_BLOCK_TEXT_SIZE`, so a line that leaves or joins a block changes its shape.
        style.font_size = px(14.).into();
        let inputs = LayoutInputs {
            buffer,
            markdown: &markdown,
            selection: &self.selection,
            marked: self.marked.as_ref(),
            style: &style,
            wrap_width: self.wrap_width,
            margin: self.margin,
            code_block_wrap: self.code_block_wrap,
            theme: self.theme,
            scrolls: &BlockScrolls::default(),
            decorations,
        };
        shape_document(&inputs, previous, window)
    }
}

fn assert_line(carried: &Placed, fresh: &Placed) -> Result<(), TestCaseError> {
    let at = carried.start;
    prop_assert_eq!(carried.start, fresh.start);
    prop_assert_eq!(carried.top, fresh.top, "top at {}", at);
    prop_assert_eq!(carried.first_row, fresh.first_row, "first row at {}", at);
    prop_assert_eq!(carried.block, fresh.block, "block at {}", at);
    let (carried, fresh) = (&*carried.line, &*fresh.line);
    prop_assert_eq!(carried.len, fresh.len, "len at {}", at);
    prop_assert_eq!(carried.row_height, fresh.row_height, "row height at {}", at);
    let rows = |line: &super::LineLayout| {
        let rows = line.rows.iter();
        rows.map(|row| (row.start, row.caret_end, row.start_x))
            .collect::<Vec<_>>()
    };
    prop_assert_eq!(rows(carried), rows(fresh), "rows at {}", at);
    prop_assert_eq!(
        format!("{:?}", carried.display),
        format!("{:?}", fresh.display),
        "display at {}",
        at
    );
    let prefix = |line: &super::LineLayout| {
        line.prefix.as_ref().map(|prefix| {
            let shown: Vec<_> = prefix
                .shown()
                .map(|(shaped, x)| (shaped.text.to_string(), x))
                .collect();
            (
                prefix.range(),
                prefix.indent(),
                prefix.bars().to_vec(),
                shown,
            )
        })
    };
    let marks = |line: &super::LineLayout| {
        let marks = line.prefix.iter().flat_map(super::prefix::Prefix::marks);
        marks
            .map(|mark| match &mark.paint {
                super::prefix::Paint::Text(shaped) => (mark.x, Some(shaped.text.to_string()), None),
                super::prefix::Paint::Box { checked } => (mark.x, None, Some(*checked)),
            })
            .collect::<Vec<_>>()
    };
    prop_assert_eq!(marks(carried), marks(fresh), "marks at {}", at);
    prop_assert_eq!(carried.task_box(), fresh.task_box(), "task box at {}", at);
    prop_assert_eq!(prefix(carried), prefix(fresh), "prefix at {}", at);
    let label = |line: &super::LineLayout| line.label.as_ref().map(|label| label.text.to_string());
    prop_assert_eq!(label(carried), label(fresh), "label at {}", at);
    prop_assert!(carried.shape.same_args(&fresh.shape), "shape at {}", at);
    Ok(())
}

fn assert_same(carried: &DocumentLayout, fresh: &DocumentLayout) -> Result<(), TestCaseError> {
    prop_assert_eq!(carried.lines.len(), fresh.lines.len());
    for (carried, fresh) in carried.lines.iter().zip(&fresh.lines) {
        assert_line(carried, fresh)?;
    }
    prop_assert_eq!(carried.row_count, fresh.row_count);
    prop_assert_eq!(carried.height, fresh.height);
    prop_assert_eq!(&carried.bars, &fresh.bars);
    let blocks = |layout: &DocumentLayout| {
        let blocks = layout.code_blocks.iter().map(|block| {
            let label = block.label.as_ref();
            let label = label.map(|(at, shaped)| (*at, shaped.text.to_string()));
            (block.start, block.max, block.background, label)
        });
        blocks.collect::<Vec<_>>()
    };
    prop_assert_eq!(blocks(carried), blocks(fresh));
    Ok(())
}

fn check(
    text: &str,
    steps: &[Step],
    window: &mut Window,
    cx: &mut App,
) -> Result<(), TestCaseError> {
    layer::declare(cx);
    let buffer = cx.new(|_| Buffer::new(text));
    let mut seen = layer::Seen::new(buffer.read(cx));
    let mut inputs = Inputs {
        selection: ByteOffset::default()..ByteOffset::default(),
        marked: None,
        wrap_width: None,
        margin: px(76.),
        code_block_wrap: false,
        theme: builtin::dark(),
    };
    let mut layout = inputs.layout(&buffer, None, window, cx);
    for step in steps {
        if let Step::Paint(paint) = step {
            layer::apply(paint, &buffer, &seen, cx);
        }
        buffer.update(cx, |buffer, _| inputs.apply(buffer, step, &layout));
        let synced = !matches!(step, Step::Unsynced(_));
        if synced {
            decorations::sync(&buffer, cx);
        }
        seen.note(buffer.read(cx));
        layout = inputs.layout(&buffer, Some(&layout), window, cx);
        assert_same(&layout, &inputs.layout(&buffer, None, window, cx))?;
        // Nothing changed since, so every line carries, after a flip too; a layer trailing the
        // buffer, drawn by neither, still rebuilds.
        let before = line_build_count();
        let again = inputs.layout(&buffer, Some(&layout), window, cx);
        if inputs.marked.is_none() && synced {
            prop_assert_eq!(line_build_count() - before, 0, "after {:?}", step);
        }
        assert_same(&again, &layout)?;
    }
    Ok(())
}

#[gpui::test]
fn a_carried_layout_matches_a_fresh_one(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let state = RefCell::new((window, cx));
        let config = Config {
            cases: 2048,
            source_file: Some(file!()),
            ..Config::default()
        };
        let strategy = (markdown(48), prop::collection::vec(step(), 1..10));
        let result = TestRunner::new(config).run(&strategy, |(text, steps)| {
            let (window, cx) = &mut *state.borrow_mut();
            check(&text, &steps, window, cx)
        });
        if let Err(failure) = result {
            panic!("{failure}");
        }
    });
}
