use super::*;

fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

fn block(a: usize, b: usize) -> (Range<ByteOffset>, u64) {
    (r(a, b), 7)
}

fn held(a: usize, b: usize) -> (Range<ByteOffset>, Option<u64>) {
    (r(a, b), Some(7))
}

#[test]
fn a_deleted_closing_fence_clears_both_old_chunks_and_lexes_the_merged_block() {
    let plan = plan(&[block(0, 30)], &[held(0, 12), held(14, 30)]);

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(0, 12), r(14, 30)],
            lex: vec![0]
        }
    );
}

/// The core keeps the chunk's range and tag through a deletion that only touches it (deviation 10).
#[test]
fn a_line_deleted_right_before_the_fence_keeps_the_blocks_chunk_and_lexes_nothing() {
    let plan = plan(&[block(0, 10)], &[held(0, 10)]);

    assert_eq!(plan, Plan::default());
}

#[test]
fn a_prose_chunk_is_cleared() {
    let plan = plan(&[], &[held(0, 5)]);

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(0, 5)],
            lex: vec![]
        }
    );
}

#[test]
fn a_chunk_with_the_same_start_and_another_end_is_cleared_and_the_block_lexed() {
    let plan = plan(&[block(0, 10)], &[held(0, 8)]);

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(0, 8)],
            lex: vec![0]
        }
    );
}

#[test]
fn a_chunk_between_blocks_is_cleared() {
    let plan = plan(
        &[block(0, 10), block(30, 40)],
        &[held(0, 10), held(15, 20), held(30, 40)],
    );

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(15, 20)],
            lex: vec![]
        }
    );
}

#[test]
fn only_the_blocks_no_chunk_holds_are_lexed() {
    let plan = plan(
        &[block(0, 10), block(20, 30), block(40, 50)],
        &[held(0, 10), held(40, 50)],
    );

    assert_eq!(
        plan,
        Plan {
            clear: vec![],
            lex: vec![1]
        }
    );
}

/// A container above can turn a block from fenced into indented code on the same range.
#[test]
fn a_chunk_on_the_blocks_range_with_another_tag_or_none_is_lexed_over() {
    for tag in [Some(8), None] {
        let plan = plan(&[block(20, 30)], &[(r(20, 30), tag)]);

        assert_eq!(
            plan,
            Plan {
                clear: vec![],
                lex: vec![0]
            },
            "{tag:?}"
        );
    }
}
