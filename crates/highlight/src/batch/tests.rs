use inkra_markdown::parse;

use super::*;
use crate::fixtures::{TWO_BLOCKS, r};

/// Each block of `markdown` held exactly, with its range and shape.
fn exact(markdown: &Markdown) -> Vec<(Range<ByteOffset>, Option<u64>)> {
    markdown
        .code_blocks
        .iter()
        .map(|block| (block.range.clone(), Some(block.shape)))
        .collect()
}

/// The core drops a chunk's tag when a splice inserts inside it.
#[test]
fn a_chunk_an_edit_left_untagged_is_lexed_over() {
    let markdown = parse(TWO_BLOCKS);
    let second = markdown.code_blocks[1].shape;
    assert_ne!(markdown.code_blocks[0].shape, second);
    let mut held = exact(&markdown);
    held[1].1 = None;

    let batch = batch(TWO_BLOCKS, &markdown, &held);

    assert_eq!(
        batch,
        Batch {
            clear: vec![],
            lexed: vec![(r(17, 32), second, vec![(r(23, 26), TokenClass::Keyword)])],
        }
    );
}

#[test]
fn a_held_chunk_on_prose_is_cleared() {
    let text = "prose\n\n```rust\nlet\n```\n";
    let markdown = parse(text);
    let mut held = vec![(r(0, 5), Some(markdown.code_blocks[0].shape))];
    held.extend(exact(&markdown));

    let batch = batch(text, &markdown, &held);

    assert_eq!(
        batch,
        Batch {
            clear: vec![r(0, 5)],
            lexed: vec![],
        }
    );
}

#[test]
fn only_the_blocks_no_chunk_matches_are_lexed() {
    let markdown = parse(TWO_BLOCKS);
    let held = &exact(&markdown)[..1];

    let batch = batch(TWO_BLOCKS, &markdown, held);

    assert_eq!(
        batch,
        Batch {
            clear: vec![],
            lexed: vec![(
                r(17, 32),
                markdown.code_blocks[1].shape,
                vec![(r(23, 26), TokenClass::Keyword)]
            )],
        }
    );
}

#[test]
fn an_unknown_language_block_gets_an_empty_chunk() {
    let text = "```nope\nlet\n```\n";
    let markdown = parse(text);

    let batch = batch(text, &markdown, &[]);

    assert_eq!(
        batch.lexed,
        [(r(0, 15), markdown.code_blocks[0].shape, vec![])]
    );
}

/// A rust block whose range is `len` bytes, fences included: `let`, then an identifier.
fn rust_block(len: usize) -> String {
    let fences = "```rust\nlet \n```".len();
    format!("```rust\nlet {}\n```\n", "x".repeat(len - fences))
}

#[test]
fn a_block_over_the_block_limit_gets_an_empty_chunk() {
    let text = rust_block(BLOCK_LIMIT + 1);
    let markdown = parse(&text);

    let batch = batch(&text, &markdown, &[]);

    let range = r(0, BLOCK_LIMIT + 1);
    assert_eq!(
        batch.lexed,
        [(range, markdown.code_blocks[0].shape, vec![])]
    );
}

#[test]
fn a_block_at_the_block_limit_is_lexed() {
    let text = rust_block(BLOCK_LIMIT);
    let markdown = parse(&text);

    let batch = batch(&text, &markdown, &[]);

    let (range, _, marks) = &batch.lexed[0];
    assert_eq!(*range, r(0, BLOCK_LIMIT));
    assert_eq!(marks[0], (r(8, 11), TokenClass::Keyword));
}

/// The limit counts the block's own bytes, not its end offset.
#[test]
fn a_small_block_ending_past_the_block_limit_is_lexed() {
    let text = format!("{}\n\n```rust\nlet\n```\n", "x".repeat(BLOCK_LIMIT));
    let markdown = parse(&text);

    let batch = batch(&text, &markdown, &[]);

    let (range, _, marks) = &batch.lexed[0];
    assert!(range.end.get() > BLOCK_LIMIT);
    assert_eq!(marks.len(), 1);
}

#[test]
fn block_marks_are_the_tokens_of_its_content_lines() {
    let text = "> ```rust\n> let\n> ```\n";
    let markdown = parse(text);

    let marks = block_marks(text, &markdown.spans, &markdown.code_blocks[0]);

    assert_eq!(marks, [(r(12, 15), TokenClass::Keyword)]);
}
