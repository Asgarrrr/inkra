use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Entity, Subscription, TestAppContext, prelude::*};
use inkra_text::{Buffer, ByteOffset, Txn};

use super::*;
use crate::editor::EditorView;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// `text` in a buffer shown in an editor, with highlighting on.
fn shown(cx: &mut TestAppContext, text: &str) -> (Entity<Buffer>, Subscription) {
    opened(cx, text, |_, _| {})
}

/// As `shown`, with `before` run on the buffer before its listener first hears it.
fn opened(
    cx: &mut TestAppContext,
    text: &str,
    before: impl FnOnce(&Entity<Buffer>, &mut App),
) -> (Entity<Buffer>, Subscription) {
    let buffer = cx.new(|_| Buffer::new(text));
    let listening = cx.update(|cx| {
        decorations::declare(MANIFEST.layers, cx);
        decorations::enable([FEATURE], cx);
        let listening = decorations::listen(SYNTAX, Rc::new(listener), cx);
        before(&buffer, cx);
        listening
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

const TWO_BLOCKS: &str = "```rust\nlet\n```\n\n```rust\nmut\n```\n";

fn lexed() -> usize {
    LEXED.with(Cell::get)
}

/// Lands the first `count` blocks of `buffer`'s text, coloured, as the open function would, with
/// its seed.
fn seed(buffer: &Entity<Buffer>, count: usize, cx: &mut App) {
    let text = buffer.read(cx).text().to_string();
    let markdown = inkra_markdown::parse(&text);
    let chunks = markdown.code_blocks[..count]
        .iter()
        .map(|block| (block.range.clone(), colour(&text, &markdown.spans, block)))
        .collect();
    let opened = decorations::Opened {
        chunks,
        seed: Some(Box::new(())),
    };
    decorations::seed(buffer, vec![(SYNTAX, opened)], cx);
}

#[gpui::test]
fn an_edit_in_prose_lexes_nothing(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "prose\n\n```rust\nlet\n```\n");
    let before = lexed();

    edit(cx, &buffer, |t| t.insert(at(0), "x"));

    assert_eq!(lexed() - before, 0);
    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

#[gpui::test]
fn an_edit_in_one_block_lexes_that_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, TWO_BLOCKS);
    let before = lexed();

    edit(cx, &buffer, |t| t.insert(at(8), "fn "));

    assert_eq!(lexed() - before, 1);
    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

#[gpui::test]
fn deleting_a_fence_indent_space_relexes_the_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "  ```rust\n  let\n  ```\n\n```rust\nmut\n```\n");
    let before = lexed();

    edit(cx, &buffer, |t| t.delete(at(0)..at(1)));

    assert_eq!(lexed() - before, 1);
    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

#[gpui::test]
fn two_edits_in_one_hearing_relex_both_blocks(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, TWO_BLOCKS);
    let before = lexed();

    buffer.update(cx, |buffer, cx| {
        buffer.transact(|t| t.insert(at(25), "fn "));
        buffer.transact(|t| t.insert(at(8), "fn "));
        cx.notify();
    });
    cx.run_until_parked();

    assert_eq!(lexed() - before, 2);
    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

#[gpui::test]
fn a_changed_info_word_recolours_its_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "```nope\nlet\n```\n");

    edit(cx, &buffer, |t| t.replace(at(3)..at(7), "rust"));

    let recoloured = fresh(&text(cx, &buffer));
    assert_ne!(recoloured, []);
    assert_eq!(marks(cx, &buffer), recoloured);
}

#[gpui::test]
fn a_typed_fence_recolours_the_blocks_after_it(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "prose\n\n```rust\nlet\n```\n");

    edit(cx, &buffer, |t| t.insert(at(0), "```\n"));

    let text = text(cx, &buffer);
    assert_eq!(marks(cx, &buffer), fresh(&text));
    let blocks: Vec<_> = inkra_markdown::parse(&text)
        .code_blocks
        .into_iter()
        .map(|block| block.range.start.get()..block.range.end.get())
        .collect();
    for (chunk, marked) in held(cx, &buffer) {
        assert!(
            !marked || blocks.contains(&chunk),
            "{chunk:?} in {blocks:?}"
        );
    }
}

#[gpui::test]
fn a_partly_seeded_open_lexes_only_the_rest(cx: &mut TestAppContext) {
    let before = lexed();

    let (buffer, _listening) = opened(cx, TWO_BLOCKS, |buffer, cx| {
        seed(buffer, 1, cx);
    });

    assert_eq!(lexed() - before, 1);
    assert_eq!(marks(cx, &buffer), fresh(TWO_BLOCKS));
}

#[gpui::test]
fn an_edit_before_the_first_hearing_relexes_every_block(cx: &mut TestAppContext) {
    let before = lexed();

    let (buffer, _listening) = opened(cx, TWO_BLOCKS, |buffer, cx| {
        seed(buffer, 2, cx);
        buffer.update(cx, |buffer, cx| {
            buffer.transact(|t| t.insert(at(8), "fn "));
            cx.notify();
        });
    });

    assert_eq!(lexed() - before, 2);
    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

#[gpui::test]
fn an_open_landed_on_prose_loses_its_marks(cx: &mut TestAppContext) {
    let (buffer, _listening) = opened(cx, TWO_BLOCKS, |buffer, cx| {
        seed(buffer, 1, cx);
        buffer.update(cx, |buffer, cx| {
            buffer.transact(|t| t.delete(at(0)..at(3)));
            cx.notify();
        });
    });

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
    assert!(
        held(cx, &buffer).iter().all(|(_, marked)| !marked),
        "{:?}",
        held(cx, &buffer)
    );
}
