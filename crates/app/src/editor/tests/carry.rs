//! A layout carries every line an edit leaves clean (incremental layout spec, D5 and D7).

use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

use gpui::{App, Entity, TestAppContext, Window, prelude::*, px};
use inkra_text::{Buffer, ByteOffset};

use super::shapes::{layout_of, reshape, shape};
use super::{at, dispatching, window_on, with_editor};
use crate::decorations::{self, Drawn, LayerDecl, LayerId, MarkStyle};
use crate::editor::layout::{
    BlockScrolls, DocumentLayout, LayoutInputs, line_build_count, shape_document,
};
use crate::feature::fakes::{self, TODO_DECL, TODO_FAKE, TODO_LAYER};
use crate::theme::{ColorToken, builtin};

/// The lines `f` builds rather than carries.
fn builds<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = line_build_count();
    let result = f();
    (result, line_build_count() - before)
}

/// `before`'s layout, then `after`'s carried from it in the same buffer, and the lines it built.
fn carried(before: &str, after: &str, window: &mut Window) -> (DocumentLayout, usize) {
    let mut buffer = Buffer::new(before);
    let previous = layout_of(&buffer, None, None, None, window);
    builds(|| reshape(&mut buffer, after, None, None, Some(&previous), window))
}

/// `buffer`'s layout with the caret at byte `caret`, carried from `previous`.
fn with_caret(
    buffer: &Buffer,
    caret: usize,
    decorations: Drawn<'_>,
    previous: Option<&DocumentLayout>,
    window: &mut Window,
) -> DocumentLayout {
    with_inputs(buffer, caret, decorations, window, |inputs, window| {
        shape_document(inputs, previous, window)
    })
}

/// `f` on `buffer`'s layout inputs with the caret at byte `caret`.
fn with_inputs<R>(
    buffer: &Buffer,
    caret: usize,
    decorations: Drawn<'_>,
    window: &mut Window,
    f: impl FnOnce(&LayoutInputs<'_>, &mut Window) -> R,
) -> R {
    let markdown = Rc::new(inkra_markdown::parse(buffer.text()));
    let style = window.text_style();
    let inputs = LayoutInputs {
        buffer,
        markdown: &markdown,
        selection: &(at(caret)..at(caret)),
        marked: None,
        style: &style,
        wrap_width: None,
        margin: px(76.),
        code_block_wrap: false,
        theme: builtin::dark(),
        scrolls: &BlockScrolls::default(),
        decorations,
    };
    f(&inputs, window)
}

/// `buffer`'s layout in its colours, the caret at its start, carried from `previous`.
fn coloured(
    buffer: &Entity<Buffer>,
    previous: Option<&DocumentLayout>,
    window: &mut Window,
    cx: &App,
) -> DocumentLayout {
    let drawn = decorations::drawn(buffer, cx);
    with_caret(buffer.read(cx), 0, drawn, previous, window)
}

/// One chunk of `layer` over `chunk`, its first three bytes in `token`, published at the buffer's
/// edit mark.
fn recolour(
    layer: LayerId,
    buffer: &Entity<Buffer>,
    chunk: &Range<ByteOffset>,
    token: ColorToken,
    cx: &mut App,
) {
    let style = MarkStyle { color: Some(token) };
    let marks = vec![(chunk.start..chunk.start + 3, style)];
    let mark = buffer.read(cx).edit_mark();
    let chunks = vec![(chunk.clone(), 0, marks)];
    decorations::publish(layer, &buffer.downgrade(), Vec::new(), chunks, mark, cx);
}

const TWO_BLOCKS: &str = "top\n\n```rust\nlet a = 1;\n```\n\nmid\n\n```rust\nlet b = 2;\n```\n";

/// `TWO_BLOCKS` in a window, both blocks coloured, and each block's range.
fn two_coloured_blocks(cx: &mut TestAppContext) -> (Entity<Buffer>, Vec<Range<ByteOffset>>) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(TWO_BLOCKS)));
    window_on(cx, buffer.clone());
    let markdown = inkra_markdown::parse(TWO_BLOCKS);
    let blocks: Vec<_> = markdown
        .code_blocks
        .iter()
        .map(|block| block.range.clone())
        .collect();
    cx.update(|cx| {
        fakes::declare_todo(cx);
        for block in &blocks {
            recolour(TODO_LAYER, &buffer, block, ColorToken::Accent, cx);
        }
    });
    cx.run_until_parked();
    (buffer, blocks)
}

/// Every caret position and every line's code block match a fresh layout of `text`.
fn assert_fresh(layout: &DocumentLayout, text: &str, window: &mut Window) {
    let fresh = shape(text, None, None, None, window);
    assert_eq!(layout.height(), fresh.height(), "{text:?}");
    assert_eq!(layout.line_blocks(), fresh.line_blocks(), "{text:?}");
    assert_eq!(
        layout.block_backgrounds(),
        fresh.block_backgrounds(),
        "{text:?}"
    );
    for offset in (0..=text.len()).filter(|&offset| text.is_char_boundary(offset)) {
        assert_eq!(
            layout.position_for_offset(at(offset)),
            fresh.position_for_offset(at(offset)),
            "{text:?} at {offset}"
        );
    }
}

#[gpui::test]
fn a_keystroke_rebuilds_only_its_line(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "# Title\n\nsome *text* here\n\nmore prose\n");
    cx.simulate_keystrokes("cmd-up down down right right");
    cx.run_until_parked();

    let ((), built) = builds(|| {
        cx.simulate_input("x");
        cx.run_until_parked();
    });
    assert_eq!(built, 1);
}

#[gpui::test]
fn a_keystroke_with_colours_rebuilds_only_its_line(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\n\nsome *text* here\n\nmore TODO prose\n");
    let buffer = window
        .update(&mut cx, |view, _, _| view.input.buffer().clone())
        .unwrap();
    let _listening = cx.update(|_, cx| {
        fakes::declare_todo(cx);
        decorations::listen(TODO_LAYER, Rc::new(fakes::todo_listener), cx)
    });
    cx.simulate_keystrokes("cmd-up down down right right");
    cx.run_until_parked();
    let drawn = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let len = buffer.read(cx).len();
            decorations::drawn(&buffer, cx).colors_in(at(0)..len).len()
        })
    };
    assert_eq!(drawn(&mut cx), 1, "the marker is coloured");

    let ((), built) = builds(|| {
        cx.simulate_input("x");
        cx.run_until_parked();
    });
    assert_eq!(built, 1);
    assert_eq!(drawn(&mut cx), 1);
}

#[gpui::test]
fn a_recolour_of_one_block_rebuilds_only_its_lines(cx: &mut TestAppContext) {
    let (buffer, blocks) = two_coloured_blocks(cx);
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let previous = coloured(&buffer, None, window, cx);
        recolour(TODO_LAYER, &buffer, &blocks[1], ColorToken::Warning, cx);
        let (layout, built) = builds(|| coloured(&buffer, Some(&previous), window, cx));
        assert_eq!(built, 3, "the block's fences and its one line");
        let fresh = coloured(&buffer, None, window, cx);
        for line in 0..TWO_BLOCKS.split('\n').count() {
            assert_eq!(
                layout.run_colors(line),
                fresh.run_colors(line),
                "line {line}"
            );
        }
    });
}

/// A layer trailing the buffer draws none of its marks, so no layout drawn then is reused, even
/// when the buffer has not moved since (S1b deviation 4).
#[gpui::test]
fn a_layout_drawn_while_a_layer_trails_is_never_reused(cx: &mut TestAppContext) {
    static LATER: LayerDecl = LayerDecl {
        id: LayerId::new(TODO_FAKE.id, "later"),
        open: None,
    };
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("aaaa\nbbbb\n")));
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        decorations::declare([&TODO_DECL, &LATER], cx);
        decorations::enable([TODO_FAKE.id], cx);
        recolour(
            TODO_LAYER,
            &buffer,
            &(at(0)..at(4)),
            ColorToken::Warning,
            cx,
        );
        recolour(LATER.id, &buffer, &(at(5)..at(9)), ColorToken::Accent, cx);
        // No sync: the first layer trails the buffer from here on.
        buffer.update(cx, |buffer, _| {
            buffer.transact(|txn| txn.insert(at(10), "c"));
        });
        recolour(LATER.id, &buffer, &(at(5)..at(9)), ColorToken::Warning, cx);
        let first = coloured(&buffer, None, window, cx);

        recolour(
            LATER.id,
            &buffer,
            &(at(5)..at(9)),
            ColorToken::SyntaxKeyword,
            cx,
        );

        let drawn = decorations::drawn(&buffer, cx);
        let reused = with_inputs(buffer.read(cx), 0, drawn, window, |inputs, _| {
            first.is_shaped_from(inputs)
        });
        assert!(!reused);
        let fresh = coloured(&buffer, None, window, cx);
        assert_ne!(first.run_colors(1), fresh.run_colors(1));
    });
}

#[gpui::test]
fn landing_a_seed_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    let (buffer, _) = two_coloured_blocks(cx);
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let previous = coloured(&buffer, None, window, cx);
        let opened = decorations::run_openers(
            TWO_BLOCKS,
            &[(TODO_LAYER, fakes::open_todo)],
            Instant::now(),
        );
        decorations::seed(&buffer, opened, cx);
        let (_, built) = builds(|| coloured(&buffer, Some(&previous), window, cx));
        assert_eq!(built, TWO_BLOCKS.split('\n').count());
    });
}

#[gpui::test]
fn switching_a_layer_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    let (buffer, _) = two_coloured_blocks(cx);
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let previous = coloured(&buffer, None, window, cx);
        decorations::enable([], cx);
        let (_, built) = builds(|| coloured(&buffer, Some(&previous), window, cx));
        assert_eq!(built, TWO_BLOCKS.split('\n').count());
    });
}

#[gpui::test]
fn a_newline_above_shifts_without_rebuilding(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "one\n\n\ntwo\n\nthree\n\nfour\n";
        let (layout, built) = carried("one\n\ntwo\n\nthree\n\nfour\n", after, window);
        assert_eq!(built, 2, "the split line and the new one");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_fence_typed_above_moves_every_block_index(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let before = "top\n\n```\na\n```\n\nmid\n\n```\nb\n```\n";
        let after = format!("```\nz\n```\n\n{before}");
        let (layout, built) = carried(before, &after, window);
        assert_eq!(
            built, 5,
            "the new block, its blank line and the line it pushed"
        );
        assert_fresh(&layout, &after, window);
    });
}

#[gpui::test]
fn opening_a_fence_rebuilds_every_line_after_it(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "a\n```\nb\n\nc\n\nd";
        let (layout, built) = carried("a\n\nb\n\nc\n\nd", after, window);
        assert_eq!(built, 6, "every line from the fence on");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_lazy_quote_line_follows_its_gutter(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "> > a\nlazy\n\nafter\n";
        let (layout, built) = carried(">  > a\nlazy\n\nafter\n", after, window);
        assert_eq!(built, 2, "the gutter line and its lazy line");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_changed_gutter_reaches_its_lazy_lines_and_stops(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "> > a\nb\nc\n\n> d\ne\n";
        let (layout, built) = carried(">  > a\nb\nc\n\n> d\ne\n", after, window);
        assert_eq!(built, 3, "the gutter line and its two lazy lines");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_whole_gutter_below_an_edit_carries(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "xy\n> a\n\nz\n";
        let (layout, built) = carried("x\n> a\n\nz\n", after, window);
        assert_eq!(built, 1);
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_conceal_below_an_edit_that_the_caret_leaves_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let mut buffer = Buffer::new("one\n\nsome *b* here");
        let revealed = with_caret(&buffer, 11, Drawn::default(), None, window);
        buffer.transact(|txn| txn.insert(at(0), "x"));
        let (_, built) =
            builds(|| with_caret(&buffer, 1, Drawn::default(), Some(&revealed), window));
        assert_eq!(built, 3);
    });
}

#[gpui::test]
fn a_conceal_below_an_edit_that_stays_revealed_carries(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let mut buffer = Buffer::new("one\n\nsome *b* here");
        let revealed = with_caret(&buffer, 11, Drawn::default(), None, window);
        buffer.transact(|txn| txn.insert(at(0), "x"));
        let (_, built) =
            builds(|| with_caret(&buffer, 12, Drawn::default(), Some(&revealed), window));
        assert_eq!(built, 1);
    });
}

#[gpui::test]
fn unmarking_rebuilds_the_marked_line(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let buffer = Buffer::new("zero\none two\nthree");
        let marked = layout_of(&buffer, Some(&(at(5)..at(8))), None, None, window);
        let (_, built) = builds(|| layout_of(&buffer, None, None, Some(&marked), window));
        assert_eq!(built, 1);
    });
}

#[gpui::test]
fn moving_the_marked_text_keeps_the_lines_between(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let buffer = Buffer::new("zero\none\ntwo\nthree\nfour");
        let marked = layout_of(&buffer, Some(&(at(19)..at(23))), None, None, window);
        let next = at(0)..at(4);
        let (_, built) = builds(|| layout_of(&buffer, Some(&next), None, Some(&marked), window));
        assert_eq!(built, 2, "the line left and the line marked");
    });
}

#[gpui::test]
fn a_layout_from_another_buffer_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let text = "zero\none\ntwo";
        let previous = shape(text, None, None, None, window);
        let (_, built) = builds(|| shape(text, None, None, Some(&previous), window));
        assert_eq!(built, 3);
    });
}

#[gpui::test]
fn an_overflowed_splice_log_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let mut buffer = Buffer::new("zero\none\ntwo");
        let previous = layout_of(&buffer, None, None, None, window);
        for _ in 0..257 {
            buffer.transact(|buffer| buffer.insert(at(0), "x"));
        }
        let (_, built) = builds(|| layout_of(&buffer, None, None, Some(&previous), window));
        assert_eq!(built, 3);
    });
}
