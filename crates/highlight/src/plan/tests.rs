use super::*;

fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

fn block(a: usize, b: usize, reach: usize) -> (Range<ByteOffset>, ByteOffset) {
    (r(a, b), ByteOffset::new(reach))
}

#[test]
fn a_deleted_closing_fence_clears_both_old_chunks_and_lexes_the_merged_block() {
    let plan = plan(
        &[block(0, 30, 0)],
        &[(r(0, 12), true), (r(14, 30), true)],
        Some(r(12, 12)),
    );

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(0, 12), r(14, 30)],
            lex: vec![0]
        }
    );
}

#[test]
fn a_line_deleted_right_before_the_fence_lexes_the_block() {
    let plan = plan(&[block(0, 10, 0)], &[(r(0, 10), true)], Some(r(0, 0)));

    assert_eq!(
        plan,
        Plan {
            clear: vec![],
            lex: vec![0]
        }
    );
}

#[test]
fn a_deletion_in_the_fence_indent_lexes_the_block() {
    let plan = plan(&[block(11, 20, 10)], &[(r(11, 20), true)], Some(r(10, 10)));

    assert_eq!(
        plan,
        Plan {
            clear: vec![],
            lex: vec![0]
        }
    );
}

#[test]
fn a_prose_chunk_without_marks_is_not_cleared_again() {
    assert_eq!(plan(&[], &[(r(0, 5), false)], None), Plan::default());
}

#[test]
fn a_prose_chunk_with_marks_is_cleared() {
    let plan = plan(&[], &[(r(0, 5), true)], None);

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
    let plan = plan(&[block(0, 10, 0)], &[(r(0, 8), true)], None);

    assert_eq!(
        plan,
        Plan {
            clear: vec![r(0, 8)],
            lex: vec![0]
        }
    );
}

#[test]
fn a_chunk_with_marks_between_blocks_is_cleared() {
    let plan = plan(
        &[block(0, 10, 0), block(30, 40, 30)],
        &[(r(0, 10), true), (r(15, 20), true), (r(30, 40), true)],
        None,
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
fn no_dirty_range_lexes_only_the_blocks_no_chunk_holds() {
    let plan = plan(
        &[block(0, 10, 0), block(20, 30, 20), block(40, 50, 40)],
        &[(r(0, 10), true), (r(40, 50), false)],
        None,
    );

    assert_eq!(
        plan,
        Plan {
            clear: vec![],
            lex: vec![1]
        }
    );
}

#[test]
fn the_dirty_range_meets_the_reach_when_it_touches_it() {
    for (dirty, lexed) in [
        (r(30, 31), true),
        (r(31, 32), false),
        (r(10, 15), true),
        (r(10, 14), false),
    ] {
        let plan = plan(
            &[block(20, 30, 15)],
            &[(r(20, 30), true)],
            Some(dirty.clone()),
        );

        let lex = if lexed { vec![0] } else { vec![] };
        assert_eq!(plan, Plan { clear: vec![], lex }, "{dirty:?}");
    }
}
