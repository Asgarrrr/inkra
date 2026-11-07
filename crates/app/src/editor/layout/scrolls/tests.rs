use std::time::Instant;

use gpui::px;
use inkra_text::{Buffer, ByteOffset};

use super::BlockScrolls;

fn starts(scrolls: &BlockScrolls) -> Vec<usize> {
    scrolls
        .0
        .borrow()
        .iter()
        .map(|scroll| scroll.start.get())
        .collect()
}

#[test]
fn a_zero_for_a_missing_block_stores_nothing() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(4), px(0.), Instant::now(), &mut buffer);
    assert!(starts(&scrolls).is_empty());
}

#[test]
fn a_zero_for_a_stored_block_keeps_its_entry() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(4), px(30.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(4), px(0.), Instant::now(), &mut buffer);
    assert_eq!(starts(&scrolls), [4]);
    assert_eq!(scrolls.x(ByteOffset::new(4)), px(0.));
}

#[test]
fn entries_stay_sorted_by_start() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(9), px(10.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(2), px(20.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(5), px(30.), Instant::now(), &mut buffer);
    assert_eq!(starts(&scrolls), [2, 5, 9]);
    assert_eq!(scrolls.x(ByteOffset::new(5)), px(30.));
    assert_eq!(scrolls.x(ByteOffset::new(3)), px(0.));
}

#[test]
fn sync_follows_an_edit_and_keeps_the_first_of_two_converging_entries() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(2), px(20.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(5), px(50.), Instant::now(), &mut buffer);
    let anchors = scrolls.anchors();
    buffer.transact(|buffer| buffer.delete(ByteOffset::new(2)..ByteOffset::new(5)));

    let dropped = scrolls.sync(&buffer, |_| true);

    assert_eq!(dropped, [anchors[1]]);
    assert_eq!(starts(&scrolls), [2]);
    assert_eq!(scrolls.x(ByteOffset::new(2)), px(20.));
}

#[test]
fn sync_drops_an_entry_that_is_no_longer_a_block_start() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(2), px(20.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(5), px(50.), Instant::now(), &mut buffer);
    let anchors = scrolls.anchors();
    buffer.transact(|buffer| buffer.insert(ByteOffset::new(0), "ab"));

    let dropped = scrolls.sync(&buffer, |start| start == ByteOffset::new(7));

    assert_eq!(dropped, [anchors[0]]);
    assert_eq!(scrolls.entries(), [(ByteOffset::new(7), px(50.))]);
}

#[test]
fn entries_omit_a_block_scrolled_back_to_zero() {
    let scrolls = BlockScrolls::default();
    let mut buffer = Buffer::new("0123456789");
    scrolls.set(ByteOffset::new(2), px(20.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(5), px(50.), Instant::now(), &mut buffer);
    scrolls.set(ByteOffset::new(2), px(0.), Instant::now(), &mut buffer);

    assert_eq!(scrolls.entries(), [(ByteOffset::new(5), px(50.))]);
    assert_eq!(scrolls.anchors().len(), 2);
    assert!(!scrolls.is_empty());
    scrolls.forget_all(&mut buffer);
    assert!(scrolls.is_empty());
}
