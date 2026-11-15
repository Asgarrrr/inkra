use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

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

fn held(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> Vec<Range<usize>> {
    cx.update(|cx| decorations::chunks(SYNTAX, buffer, cx))
        .into_iter()
        .map(|range| range.start.get()..range.end.get())
        .collect()
}

/// Each code block's range in `text`.
fn blocks(text: &str) -> Vec<Range<usize>> {
    inkra_markdown::parse(text)
        .code_blocks
        .into_iter()
        .map(|block| block.range.start.get()..block.range.end.get())
        .collect()
}

fn text(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> String {
    buffer.read_with(cx, |buffer, _| buffer.text().to_string())
}

#[gpui::test]
fn each_block_is_its_own_chunk_and_prose_holds_none(cx: &mut TestAppContext) {
    let note = PROSE_RUST_NOPE;
    let (buffer, _listening) = shown(cx, note);

    let blocks = blocks(note);
    assert_eq!(blocks.len(), 2);
    assert_eq!(held(cx, &buffer), blocks);
}

#[gpui::test]
fn a_deleted_opening_fence_leaves_no_mark(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "```rust\nlet\n```\n");
    assert_ne!(marks(cx, &buffer), []);

    edit(cx, &buffer, |t| t.delete(at(0)..at(3)));

    let text = text(cx, &buffer);
    assert_eq!(marks(cx, &buffer), fresh(&text));
    assert_eq!(held(cx, &buffer), blocks(&text));
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
    assert_eq!(held(cx, &buffer), blocks(&text));
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

    let text = text(cx, &buffer);
    assert_eq!(marks(cx, &buffer), fresh(&text));
    assert_eq!(held(cx, &buffer), blocks(&text));
}

const PROSE_RUST_NOPE: &str = "prose\n\n```rust\nlet\n```\n\n```nope\nx\n```\n";

fn far() -> Instant {
    Instant::now() + Duration::from_secs(3600)
}

/// A rust block, then `x` up to `len` bytes.
fn padded(len: usize) -> String {
    let mut text = String::from("```rust\nlet\n```\n");
    text.extend(std::iter::repeat_n('x', len - text.len()));
    text
}

#[test]
fn open_colours_every_block_before_its_deadline() {
    let markdown = inkra_markdown::parse(PROSE_RUST_NOPE);
    let [a, b] = &markdown.code_blocks[..] else {
        panic!("{:?}", markdown.code_blocks);
    };

    let opened = open::open(PROSE_RUST_NOPE, far());

    assert_eq!(
        opened.chunks,
        [
            (a.range.clone(), colour(PROSE_RUST_NOPE, &markdown.spans, a)),
            (b.range.clone(), Vec::new()),
        ]
    );
    assert!(opened.seed.is_some());
}

#[test]
fn open_past_its_deadline_lands_only_the_seed() {
    let past = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();

    let opened = open::open(PROSE_RUST_NOPE, past);

    assert_eq!(opened.chunks, []);
    assert!(opened.seed.is_some());
}

#[test]
fn a_text_over_the_parse_limit_lands_only_the_seed() {
    let opened = open::open(&padded(open::OPEN_PARSE_LIMIT + 1), far());

    assert_eq!(opened.chunks, []);
    assert!(opened.seed.is_some());
}

#[test]
fn a_text_at_the_parse_limit_is_coloured() {
    let opened = open::open(&padded(open::OPEN_PARSE_LIMIT), far());

    let [(_, marks)] = &opened.chunks[..] else {
        panic!("{} chunks", opened.chunks.len());
    };
    assert_ne!(marks, &[]);
}

#[gpui::test]
fn a_seeded_open_lexes_nothing_on_its_first_hearing(cx: &mut TestAppContext) {
    let before = lexed();

    let (buffer, _listening) = opened(cx, PROSE_RUST_NOPE, |buffer, cx| {
        let opened = open::open(PROSE_RUST_NOPE, far());
        decorations::seed(buffer, vec![(SYNTAX, opened)], cx);
    });

    assert_eq!(lexed() - before, 0);
    assert_eq!(marks(cx, &buffer), fresh(PROSE_RUST_NOPE));
    assert_eq!(held(cx, &buffer), blocks(PROSE_RUST_NOPE));
}

/// A list item's marker line decides whether the fence under it is a fence or indented code.
#[gpui::test]
fn deleting_a_list_marker_turns_a_fenced_block_into_indented_code(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "- a\n\n    ```rust\n    let\n    ```\n");
    assert_ne!(marks(cx, &buffer), []);

    edit(cx, &buffer, |t| t.delete(at(0)..at(2)));

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

/// The item's marker sits alone on the line before, so no block precedes the indented code.
#[gpui::test]
fn deleting_a_lone_list_marker_leaves_no_mark_on_the_indented_code(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "-\n    ```rust\n    let\n    ```\n");
    assert_ne!(marks(cx, &buffer), []);

    edit(cx, &buffer, |t| t.delete(at(0)..at(1)));

    assert_eq!(marks(cx, &buffer), []);
}

#[gpui::test]
fn typing_a_list_marker_turns_indented_code_into_a_fenced_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "a\n\n    ```rust\n    let\n    ```\n");

    edit(cx, &buffer, |t| t.insert(at(0), "- "));

    let coloured = fresh(&text(cx, &buffer));
    assert_ne!(coloured, []);
    assert_eq!(marks(cx, &buffer), coloured);
}

fn undo(cx: &mut TestAppContext, buffer: &Entity<Buffer>) {
    buffer.update(cx, |buffer, cx| {
        buffer.undo();
        cx.notify();
    });
    cx.run_until_parked();
}

/// `<div>` opens an HTML block that swallows the fence under it.
const UNDER_PROSE: &str = "foo\n```rust\nlet\n```\n";

#[gpui::test]
fn a_block_swallowed_by_an_html_block_leaves_no_chunk(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, UNDER_PROSE);

    edit(cx, &buffer, |t| t.insert(at(0), "<div>"));

    let none: Vec<Range<usize>> = Vec::new();
    assert_eq!(blocks(&text(cx, &buffer)), none);
    assert_eq!(held(cx, &buffer), none);
}

#[gpui::test]
fn a_block_back_from_an_html_block_is_coloured(cx: &mut TestAppContext) {
    for back in [
        (|cx: &mut TestAppContext, buffer: &Entity<Buffer>| {
            edit(cx, buffer, |t| t.delete(at(0)..at(5)));
        }) as fn(&mut TestAppContext, &Entity<Buffer>),
        undo,
    ] {
        let (buffer, _listening) = shown(cx, UNDER_PROSE);
        edit(cx, &buffer, |t| t.insert(at(0), "<div>"));

        back(cx, &buffer);

        assert_eq!(text(cx, &buffer), UNDER_PROSE);
        assert_ne!(marks(cx, &buffer), []);
        assert_eq!(marks(cx, &buffer), fresh(UNDER_PROSE));
    }
}

#[gpui::test]
fn closing_a_comment_over_two_blocks_colours_both(cx: &mut TestAppContext) {
    let note = "a\n```rust\nlet\n```\n\n```rust\nmut\n```\n";
    let (buffer, _listening) = shown(cx, note);
    edit(cx, &buffer, |t| t.insert(at(0), "<!--"));
    assert_eq!(marks(cx, &buffer), []);

    edit(cx, &buffer, |t| t.insert(at(5), "-->"));

    let text = text(cx, &buffer);
    assert_eq!(blocks(&text).len(), 2);
    assert_eq!(marks(cx, &buffer), fresh(&text));
}

/// A block with no known language holds an unmarked chunk; it must not outlive the block either.
#[gpui::test]
fn a_block_renamed_inside_an_html_block_is_coloured_when_it_comes_back(cx: &mut TestAppContext) {
    let (buffer, _listening) = shown(cx, "foo\n```nope\nlet\n```\n");
    edit(cx, &buffer, |t| t.insert(at(0), "<div>"));
    edit(cx, &buffer, |t| t.replace(at(12)..at(16), "rust"));

    edit(cx, &buffer, |t| t.delete(at(0)..at(5)));

    assert_eq!(text(cx, &buffer), UNDER_PROSE);
    assert_eq!(marks(cx, &buffer), fresh(UNDER_PROSE));
}

/// The real open function, as the editor reads it from the declared layers.
#[gpui::test]
fn the_declared_open_function_lands_colours_the_first_hearing_keeps(cx: &mut TestAppContext) {
    let buffer = cx.new(|_| Buffer::new(PROSE_RUST_NOPE));
    let before = lexed();
    let listening = cx.update(|cx| {
        decorations::declare(MANIFEST.layers, cx);
        decorations::enable([FEATURE], cx);
        let opened = decorations::run_openers(PROSE_RUST_NOPE, &decorations::openers(cx), far());
        decorations::seed(&buffer, opened, cx);
        decorations::listen(SYNTAX, Rc::new(listener), cx)
    });
    let view = buffer.clone();
    cx.add_window(|_, cx| EditorView::new(view, cx));
    cx.run_until_parked();

    assert_eq!(lexed() - before, 0);
    assert_eq!(marks(cx, &buffer), fresh(PROSE_RUST_NOPE));
    drop(listening);
}
