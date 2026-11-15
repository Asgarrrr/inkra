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
        .flat_map(|block| inkra_highlight::block_marks(text, &markdown.spans, block))
        .map(|(range, class)| (range.start.get(), range.end.get(), token(class)))
        .collect()
}

fn held(cx: &mut TestAppContext, buffer: &Entity<Buffer>) -> Vec<Range<usize>> {
    cx.update(|cx| decorations::chunks(SYNTAX, buffer, cx))
        .into_iter()
        .map(|(range, _)| range.start.get()..range.end.get())
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

/// What a planted mark paints: a block the listener lexes shows its real colours instead.
const PLANTED: ColorToken = ColorToken::SyntaxComment;

fn plant(opened: &mut decorations::Opened) {
    for (_, _, marks) in &mut opened.chunks {
        for (_, style) in marks {
            style.color = Some(PLANTED);
        }
    }
}

/// The colours of `text` once the blocks at `relexed` are lexed and every other one is planted.
fn kept(text: &str, relexed: &[usize]) -> Vec<(usize, usize, ColorToken)> {
    let markdown = inkra_markdown::parse(text);
    let fresh = fresh(text);
    assert!(fresh.iter().any(|&(_, _, token)| token != PLANTED));
    fresh
        .into_iter()
        .map(|(start, end, token)| {
            let index = markdown
                .code_blocks
                .iter()
                .position(|block| block.range.start.get() <= start && end <= block.range.end.get());
            let token = if index.is_some_and(|index| relexed.contains(&index)) {
                token
            } else {
                PLANTED
            };
            (start, end, token)
        })
        .collect()
}

/// Lands the first `count` blocks of `buffer`'s text, planted, as the open function would, with
/// its seed.
fn seed(buffer: &Entity<Buffer>, count: usize, cx: &mut App) {
    let text = buffer.read(cx).text().to_string();
    let markdown = inkra_markdown::parse(&text);
    let chunks = markdown.code_blocks[..count]
        .iter()
        .map(|block| {
            let marks = inkra_highlight::block_marks(&text, &markdown.spans, block);
            styled((block.range.clone(), block.shape, marks))
        })
        .collect();
    let mut opened = decorations::Opened {
        chunks,
        seed: Some(Box::new(())),
    };
    plant(&mut opened);
    decorations::seed(buffer, vec![(SYNTAX, opened)], cx);
}

/// As `shown`, every block planted by `seed`.
fn seeded(cx: &mut TestAppContext, text: &str) -> (Entity<Buffer>, Subscription) {
    let count = blocks(text).len();
    opened(cx, text, |buffer, cx| seed(buffer, count, cx))
}

#[gpui::test]
fn an_edit_in_prose_lexes_nothing(cx: &mut TestAppContext) {
    let (buffer, _listening) = seeded(cx, "prose\n\n```rust\nlet\n```\n");

    edit(cx, &buffer, |t| t.insert(at(0), "x"));

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[]));
}

#[gpui::test]
fn an_edit_in_one_block_lexes_that_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = seeded(cx, TWO_BLOCKS);

    edit(cx, &buffer, |t| t.insert(at(8), "fn "));

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[0]));
}

#[gpui::test]
fn deleting_a_fence_indent_space_relexes_the_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = seeded(cx, "  ```rust\n  let\n  ```\n\n```rust\nmut\n```\n");

    edit(cx, &buffer, |t| t.delete(at(0)..at(1)));

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[0]));
}

#[gpui::test]
fn two_edits_in_one_hearing_relex_both_blocks(cx: &mut TestAppContext) {
    let (buffer, _listening) = seeded(cx, TWO_BLOCKS);

    buffer.update(cx, |buffer, cx| {
        buffer.transact(|t| t.insert(at(25), "fn "));
        buffer.transact(|t| t.insert(at(8), "fn "));
        cx.notify();
    });
    cx.run_until_parked();

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[0, 1]));
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
    let (buffer, _listening) = opened(cx, TWO_BLOCKS, |buffer, cx| {
        seed(buffer, 1, cx);
    });

    assert_eq!(marks(cx, &buffer), kept(TWO_BLOCKS, &[1]));
}

#[gpui::test]
fn an_edit_before_the_first_hearing_relexes_every_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = opened(cx, TWO_BLOCKS, |buffer, cx| {
        seed(buffer, 2, cx);
        buffer.update(cx, |buffer, cx| {
            buffer.transact(|t| t.insert(at(8), "fn "));
            cx.notify();
        });
    });

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[0, 1]));
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

#[gpui::test]
fn a_seeded_open_lexes_nothing_on_its_first_hearing(cx: &mut TestAppContext) {
    let (buffer, _listening) = opened(cx, PROSE_RUST_NOPE, |buffer, cx| {
        let mut opened = open::open(PROSE_RUST_NOPE, far());
        plant(&mut opened);
        decorations::seed(buffer, vec![(SYNTAX, opened)], cx);
    });

    assert_eq!(marks(cx, &buffer), kept(PROSE_RUST_NOPE, &[]));
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
    let listening = cx.update(|cx| {
        decorations::declare(MANIFEST.layers, cx);
        decorations::enable([FEATURE], cx);
        let mut opened =
            decorations::run_openers(PROSE_RUST_NOPE, &decorations::openers(cx), far());
        for (_, opened) in &mut opened {
            plant(opened);
        }
        decorations::seed(&buffer, opened, cx);
        decorations::listen(SYNTAX, Rc::new(listener), cx)
    });
    let view = buffer.clone();
    cx.add_window(|_, cx| EditorView::new(view, cx));
    cx.run_until_parked();

    assert_eq!(marks(cx, &buffer), kept(PROSE_RUST_NOPE, &[]));
    drop(listening);
}

/// One top-level bullet, nested prose, then rust blocks: the outline note of the spec's D7.
const OUTLINE: &str = "- Project\n  - a long line of prose about the plan\n  ```rust\n  let a = 1;\n  ```\n  - more prose\n  ```rust\n  let b = 2;\n  ```\n";

#[gpui::test]
fn a_keystroke_in_nested_prose_lexes_no_block(cx: &mut TestAppContext) {
    let (buffer, _listening) = seeded(cx, OUTLINE);

    edit(cx, &buffer, |t| t.insert(at(20), "x"));

    assert_eq!(marks(cx, &buffer), kept(&text(cx, &buffer), &[]));
}

/// Each edit lies outside the block, which keeps its range while a container above turns it from
/// fenced into indented code or back.
#[gpui::test]
fn a_block_whose_container_changes_on_the_same_range_is_recoloured(cx: &mut TestAppContext) {
    for (note, removed, inserted) in [
        // The deleted lines ended both items: the block joins the inner one.
        (
            "- a\n  - b\n\n    c\n\nd\n\n    ```rust\n    let\n    ```\n",
            16..19,
            "",
        ),
        // The fence two lines above the inner marker swallowed it.
        (
            "- ```\n  z\n  - b\nfoo\n\n    ```rust\n    let\n    ```\n",
            2..5,
            "a",
        ),
        // The marker sits above the block before this one.
        ("- a\n\n  b\n\n    ```rust\n    let\n    ```\n", 0..2, ""),
        (
            "- a\n  z\n  - b\nfoo\n\n    ```rust\n    let\n    ```\n",
            2..3,
            "```",
        ),
    ] {
        let (buffer, _listening) = shown(cx, note);
        let before = marks(cx, &buffer);

        edit(cx, &buffer, |t| {
            t.replace(at(removed.start)..at(removed.end), inserted);
        });

        let text = text(cx, &buffer);
        assert_ne!(fresh(&text), before, "{note:?}");
        assert_eq!(marks(cx, &buffer), fresh(&text), "{note:?}");
    }
}
