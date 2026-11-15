use inkra_text::{Buffer, ByteOffset};

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

#[test]
fn a_new_log_answers_its_revision_with_nothing() {
    let log = RevisionLog::new();
    let buffer = Buffer::new("abc");

    assert_eq!(log.revision(), 0);
    assert_eq!(log.changed_since(0, &buffer), Some(Vec::new()));
}

#[test]
fn a_shift_changes_nothing_and_a_whole_answers_none() {
    let buffer = Buffer::new("abc");
    let mut log = RevisionLog::new();
    log.bump(3, Moved::Shift);
    assert_eq!(log.revision(), 3);
    assert_eq!(log.changed_since(0, &buffer), Some(Vec::new()));

    log.bump(5, Moved::Whole);
    log.bump(6, Moved::Shift);

    assert_eq!(log.changed_since(0, &buffer), None);
    assert_eq!(log.changed_since(3, &buffer), None);
    assert_eq!(log.changed_since(5, &buffer), Some(Vec::new()));
    assert_eq!(log.changed_since(6, &buffer), Some(Vec::new()));
}

#[test]
fn a_range_follows_the_splices_since_its_mark() {
    let mut buffer = Buffer::new("abcdef");
    let mut log = RevisionLog::new();
    log.bump(1, Moved::Range(buffer.edit_mark(), at(2)..at(4)));
    buffer.transact(|buffer| buffer.insert(at(0), "xx"));
    buffer.transact(|buffer| buffer.insert(at(8), "y"));

    assert_eq!(log.changed_since(0, &buffer), Some(vec![at(4)..at(6)]));
}

#[test]
fn text_typed_at_a_range_s_edges_joins_it() {
    let mut buffer = Buffer::new("abcdef");
    let mut log = RevisionLog::new();
    log.bump(1, Moved::Range(buffer.edit_mark(), at(2)..at(4)));
    buffer.transact(|buffer| buffer.insert(at(4), "y"));
    buffer.transact(|buffer| buffer.insert(at(2), "x"));

    assert_eq!(log.changed_since(0, &buffer), Some(vec![at(2)..at(6)]));
}

#[test]
fn every_range_after_the_revision_answers_and_none_before_it() {
    let buffer = Buffer::new("abcdef");
    let mut log = RevisionLog::new();
    let mark = buffer.edit_mark();
    log.bump(1, Moved::Range(mark, at(0)..at(1)));
    log.bump(2, Moved::Range(mark, at(2)..at(3)));
    log.bump(3, Moved::Range(mark, at(4)..at(5)));

    assert_eq!(
        log.changed_since(1, &buffer),
        Some(vec![at(2)..at(3), at(4)..at(5)])
    );
}

#[test]
fn a_range_whose_splices_the_buffer_dropped_answers_none() {
    let mut buffer = Buffer::new("abc");
    let mut log = RevisionLog::new();
    log.bump(1, Moved::Range(buffer.edit_mark(), at(0)..at(1)));
    buffer.transact(|buffer| {
        for _ in 0..257 {
            buffer.insert(at(0), "x");
        }
    });

    assert_eq!(log.changed_since(0, &buffer), None);
}

#[test]
fn a_revision_the_log_never_held_answers_none() {
    let buffer = Buffer::new("abc");
    let mut log = RevisionLog::new();
    log.bump(5, Moved::Shift);
    log.bump(9, Moved::Shift);

    assert_eq!(log.changed_since(7, &buffer), None);
    assert_eq!(log.changed_since(10, &buffer), None);
}

#[test]
fn a_revision_the_log_dropped_answers_none_and_a_recent_one_answers() {
    let buffer = Buffer::new("abc");
    let mut log = RevisionLog::new();
    for revision in 1..=1000 {
        log.bump(revision, Moved::Shift);
    }

    assert_eq!(log.changed_since(0, &buffer), None);
    assert_eq!(log.changed_since(1, &buffer), None);
    assert_eq!(log.changed_since(990, &buffer), Some(Vec::new()));
}

#[test]
#[should_panic(expected = "is not past")]
fn a_bump_must_move_the_revision_forward() {
    let mut log = RevisionLog::new();
    log.bump(2, Moved::Shift);
    log.bump(2, Moved::Shift);
}
