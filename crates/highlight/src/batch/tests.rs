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

#[test]
fn a_dirty_range_inside_one_block_lexes_only_that_block() {
    let markdown = parse(TWO_BLOCKS);
    let second = markdown.code_blocks[1].shape;
    assert_ne!(markdown.code_blocks[0].shape, second);

    let batch = batch(TWO_BLOCKS, &markdown, &exact(&markdown), Some(r(24, 25)));

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

    let batch = batch(text, &markdown, &held, None);

    assert_eq!(
        batch,
        Batch {
            clear: vec![r(0, 5)],
            lexed: vec![],
        }
    );
}

#[test]
fn no_dirty_range_lexes_only_the_blocks_no_chunk_matches() {
    let markdown = parse(TWO_BLOCKS);
    let held = &exact(&markdown)[..1];

    let batch = batch(TWO_BLOCKS, &markdown, held, None);

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
fn block_marks_are_the_tokens_of_its_content_lines() {
    let text = "> ```rust\n> let\n> ```\n";
    let markdown = parse(text);

    let marks = block_marks(text, &markdown.spans, &markdown.code_blocks[0]);

    assert_eq!(marks, [(r(12, 15), TokenClass::Keyword)]);
}
