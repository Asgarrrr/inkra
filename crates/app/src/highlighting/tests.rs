use std::ops::Range;
use std::rc::Rc;

use gpui::{Entity, Subscription, TestAppContext, prelude::*};
use inkra_text::{Buffer, ByteOffset, Txn};

use super::*;
use crate::editor::EditorView;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// `text` in a buffer shown in an editor, with highlighting on.
fn shown(cx: &mut TestAppContext, text: &str) -> (Entity<Buffer>, Subscription) {
    let buffer = cx.new(|_| Buffer::new(text));
    let listening = cx.update(|cx| {
        decorations::declare(MANIFEST.layers, cx);
        decorations::enable([FEATURE], cx);
        decorations::listen(SYNTAX, Rc::new(listener), cx)
    });
    let view = buffer.clone();
    cx.add_window(|_, cx| EditorView::new(view, cx));
    cx.run_until_parked();
    (buffer, listening)
}

fn edit(cx: &mut TestAppContext, buffer: &Entity<Buffer>, change: impl FnOnce(&mut Txn<'_>)) {
    buffer.update(cx, |buffer, cx| {
        buffer.transact(change);
        cx.notify();
    });
    cx.run_until_parked();
}

fn marks(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> Vec<(usize, usize, ColorToken)> {
    cx.update(|cx| {
        let len = buffer.read(cx).len();
        decorations::drawn(buffer, cx)
            .colors_in(at(0)..len)
            .into_iter()
            .map(|(range, token)| (range.start.get(), range.end.get(), token))
            .collect()
    })
}

/// The colours a whole lex of `text` gives.
fn fresh(text: &str) -> Vec<(usize, usize, ColorToken)> {
    let markdown = inkra_markdown::parse(text);
    markdown
        .code_blocks
        .iter()
        .flat_map(|block| colour(text, &markdown.spans, block))
        .filter_map(|(range, style)| Some((range.start.get(), range.end.get(), style.color?)))
        .collect()
}

fn held(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> Vec<(Range<usize>, bool)> {
    cx.update(|cx| decorations::chunks(SYNTAX, buffer, cx))
        .into_iter()
        .map(|(range, marked)| (range.start.get()..range.end.get(), marked))
        .collect()
}

fn text(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> String {
    buffer.read_with(cx, |buffer, _| buffer.text().to_string())
}

#[gpui::test]
fn each_block_is_its_own_chunk_and_prose_holds_none(cx: &mut TestAppContext) {
    let note = "prose\n\n```rust\nlet\n```\n\n```nope\nx\n```\n";
    let (buffer, _listening) = shown(cx, note);

    let blocks: Vec<_> = inkra_markdown::parse(note)
        .code_blocks
        .into_iter()
        .map(|block| block.range.start.get()..block.range.end.get())
        .zip([true, false])
        .collect();
    assert_eq!(blocks.len(), 2);
    assert_eq!(held(cx, &buffer), blocks);
}

#[gpui::test]
fn a_deleted_opening_fence_leaves_no_mark(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "```rust\nlet\n```\n");
    assert_ne!(marks(cx, &buffer), []);

    edit(cx, &buffer, |t| t.delete(at(0)..at(3)));

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
    assert!(
        held(cx, &buffer).iter().all(|(_, marked)| !marked),
        "{:?}",
        held(cx, &buffer)
    );
}

#[gpui::test]
fn merging_two_blocks_colours_the_merged_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "```rust\nlet\n```\n\n```rust\nmut\n```\n");

    edit(cx, &buffer, |t| t.delete(at(12)..at(15)));

    let merged = fresh(&text(cx, &buffer));
    assert_ne!(merged, []);
    assert_eq!(marks(cx, &buffer), merged);
}
