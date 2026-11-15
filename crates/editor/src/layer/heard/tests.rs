use inkra_text::{Buffer, ByteOffset};

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// One chunk over the first two bytes, marked.
fn marked() -> Vec<Published<char>> {
    vec![(at(0)..at(2), 0, vec![(at(0)..at(1), 'a')])]
}

/// More splices than the log keeps.
fn overflow(buffer: &mut Buffer) {
    buffer.transact(|buffer| {
        for _ in 0..257 {
            buffer.insert(at(0), "x");
        }
    });
}

#[test]
fn a_failed_publish_after_hearing_is_heard_lost_once() {
    let mut buffer = Buffer::new("abc");
    let old = buffer.edit_mark();
    overflow(&mut buffer);
    let mut layer = HeardLayer::<char>::new(buffer.edit_mark());
    layer.hear(&buffer);

    assert_eq!(layer.publish(&buffer, Vec::new(), marked(), old), Err(Lost));

    assert_eq!(layer.hear(&buffer), Some(TextChange::Lost));
    assert_eq!(layer.hear(&buffer), None);
}

#[test]
fn a_stale_late_publish_before_the_first_hearing_makes_the_open_stale() {
    let mut buffer = Buffer::new("abc");
    let old = buffer.edit_mark();
    overflow(&mut buffer);
    let mut layer = HeardLayer::new(buffer.edit_mark());
    layer.land(&buffer, marked());

    assert_eq!(layer.publish(&buffer, Vec::new(), marked(), old), Err(Lost));

    assert!(layer.layer().is_empty());
    assert_eq!(
        layer.hear(&buffer),
        Some(TextChange::Opened { fresh: false })
    );
}

#[test]
fn an_open_is_fresh_until_an_edit_follows_its_landing() {
    for edited in [false, true] {
        let mut buffer = Buffer::new("abc");
        let mut layer = HeardLayer::new(buffer.edit_mark());
        assert!(layer.land(&buffer, marked()));
        if edited {
            buffer.transact(|buffer| buffer.insert(at(3), "d"));
        }

        let fresh = !edited;
        assert_eq!(layer.hear(&buffer), Some(TextChange::Opened { fresh }));
    }
}

#[test]
fn a_switch_is_heard_lost_even_after_a_landing() {
    let buffer = Buffer::new("abc");
    let mut layer = HeardLayer::new(buffer.edit_mark());
    layer.land(&buffer, marked());

    layer.switch(true);

    assert_eq!(layer.hear(&buffer), Some(TextChange::Lost));
}

#[test]
fn only_a_switch_off_clears_and_it_reports_a_non_empty_layer() {
    let buffer = Buffer::new("abc");
    let mut layer = HeardLayer::new(buffer.edit_mark());
    assert!(!layer.switch(false));
    layer.land(&buffer, marked());

    assert!(!layer.switch(true));
    assert!(!layer.layer().is_empty());
    assert!(layer.switch(false));
    assert!(layer.layer().is_empty());
}

#[test]
fn catch_up_reports_a_non_empty_layer_behind_the_buffer() {
    let mut buffer = Buffer::new("abc");
    let mut layer = HeardLayer::new(buffer.edit_mark());
    buffer.transact(|buffer| buffer.insert(at(3), "d"));
    assert_eq!(layer.catch_up(&buffer), Ok(false), "empty");
    layer.land(&buffer, marked());
    assert_eq!(layer.catch_up(&buffer), Ok(false), "at the edit mark");

    buffer.transact(|buffer| buffer.insert(at(3), "e"));

    assert_eq!(layer.catch_up(&buffer), Ok(true));
    assert_eq!(layer.layer().at(), buffer.edit_mark());
}

#[test]
fn catch_up_past_the_log_is_lost_and_clears_the_layer() {
    let mut buffer = Buffer::new("abc");
    let mut layer = HeardLayer::new(buffer.edit_mark());
    layer.land(&buffer, marked());
    overflow(&mut buffer);

    assert_eq!(layer.catch_up(&buffer), Err(Lost));
    assert!(layer.layer().is_empty());
    assert_eq!(layer.catch_up(&buffer), Ok(false));
}

#[test]
fn a_publish_returns_the_range_it_recoloured() {
    let buffer = Buffer::new("abcdef");
    let mut layer = HeardLayer::new(buffer.edit_mark());
    layer.land(&buffer, marked());
    let recoloured = vec![(
        at(0)..at(6),
        0,
        vec![(at(0)..at(1), 'a'), (at(4)..at(5), 'b')],
    )];

    let published = layer.publish(&buffer, Vec::new(), recoloured.clone(), buffer.edit_mark());
    let again = layer.publish(&buffer, Vec::new(), recoloured, buffer.edit_mark());

    assert_eq!(published, Ok(Some(at(4)..at(5))));
    assert_eq!(again, Ok(None));
}

#[test]
fn landing_the_marks_held_reports_no_change() {
    let buffer = Buffer::new("abc");
    let mut layer = HeardLayer::new(buffer.edit_mark());

    assert!(layer.land(&buffer, marked()));
    assert!(!layer.land(&buffer, marked()));
}
