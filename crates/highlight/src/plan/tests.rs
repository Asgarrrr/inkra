use super::*;

fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

fn block(a: usize, b: usize, reach: usize) -> (Range<ByteOffset>, ByteOffset) {
    (r(a, b), ByteOffset::new(reach))
}

#[test]
fn a_deleted_closing_fence_clears_both_old_chunks_and_lexes_the_merged_block() {
    let plan = plan(&[block(0, 30, 0)], &[r(0, 12), r(14, 30)], Some(r(12, 12)));

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
    let plan = plan(&[block(0, 10, 0)], &[r(0, 10)], Some(r(0, 0)));

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
    let plan = plan(&[block(11, 20, 10)], &[r(11, 20)], Some(r(10, 10)));

    assert_eq!(
        plan,
        Plan {
            clear: vec![],
            lex: vec![0]
        }
    );
}

#[test]
fn a_prose_chunk_is_cleared() {
    let plan = plan(&[], &[r(0, 5)], None);

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
    let plan = plan(&[block(0, 10, 0)], &[r(0, 8)], None);

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
        &[block(0, 10, 0), block(30, 40, 30)],
        &[r(0, 10), r(15, 20), r(30, 40)],
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
        &[r(0, 10), r(40, 50)],
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
        let plan = plan(&[block(20, 30, 15)], &[r(20, 30)], Some(dirty.clone()));

        let lex = if lexed { vec![0] } else { vec![] };
        assert_eq!(plan, Plan { clear: vec![], lex }, "{dirty:?}");
    }
}

#[test]
fn dirty_folds_the_splices_into_one_range_in_the_last_text() {
    let splice = |at, removed_len, inserted_len| Splice {
        at: ByteOffset::new(at),
        removed_len,
        inserted_len,
    };
    let range_10_20 = splice(10, 0, 10);
    for (splices, folded) in [
        (vec![], None),
        (vec![splice(10, 0, 3)], Some(r(10, 13))),
        (vec![splice(10, 4, 0)], Some(r(10, 10))),
        (vec![splice(200, 0, 5), splice(0, 0, 100)], Some(r(0, 305))),
        (vec![range_10_20, splice(15, 10, 0)], Some(r(10, 15))),
        (vec![range_10_20, splice(5, 20, 3)], Some(r(5, 8))),
    ] {
        assert_eq!(dirty(splices.clone()), folded, "{splices:?}");
    }
}
