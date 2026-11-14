use inkra_text::{Buffer, ByteOffset};

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

fn span(range: Range<usize>) -> Range<ByteOffset> {
    at(range.start)..at(range.end)
}

fn marks(layer: &Layer<char>, range: Range<usize>) -> Vec<(Range<usize>, char)> {
    layer
        .marks_in(span(range))
        .map(|(range, &style)| (range.start.get()..range.end.get(), style))
        .collect()
}

fn chunks(layer: &Layer<char>) -> Vec<Range<usize>> {
    layer
        .chunks
        .iter()
        .map(|chunk| chunk.start.get()..chunk.end().get())
        .collect()
}

fn publish(
    layer: &mut Layer<char>,
    buffer: &Buffer,
    chunk: Range<usize>,
    marks: &[(Range<usize>, char)],
) -> Result<bool, Lost> {
    let marks = marks
        .iter()
        .map(|(range, style)| (span(range.clone()), *style))
        .collect();
    layer.publish(buffer, span(chunk), marks, buffer.edit_mark())
}

fn twenty() -> Buffer {
    Buffer::new("abcdefghijklmnopqrst")
}

#[test]
fn a_publish_replaces_the_marks_inside_its_chunk_only() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    let first = [(2..4, 'a'), (6..9, 'b'), (12..15, 'c')];
    assert_eq!(publish(&mut layer, &buffer, 0..20, &first), Ok(true));

    assert_eq!(
        publish(&mut layer, &buffer, 5..10, &[(7..8, 'd')]),
        Ok(true)
    );

    assert_eq!(
        marks(&layer, 0..20),
        [(2..4, 'a'), (7..8, 'd'), (12..15, 'c')]
    );
}

#[test]
fn an_identical_publish_changes_nothing() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    let marks = [(2..4, 'a'), (6..9, 'b')];
    assert_eq!(publish(&mut layer, &buffer, 0..10, &marks), Ok(true));

    assert_eq!(publish(&mut layer, &buffer, 0..10, &marks), Ok(false));
}

#[test]
fn marks_in_clips_to_the_range() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    let first = [(2..8, 'a'), (10..12, 'b'), (14..16, 'c')];
    publish(&mut layer, &buffer, 0..20, &first).unwrap();

    assert_eq!(marks(&layer, 4..11), [(4..8, 'a'), (10..11, 'b')]);
}

#[test]
#[should_panic(expected = "outside chunk")]
fn a_mark_outside_its_chunk_panics() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());

    let _ = publish(&mut layer, &buffer, 0..5, &[(3..8, 'a')]);
}

#[test]
fn marks_in_skips_a_mark_that_only_touches_the_range_or_holds_an_empty_one() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..20, &[(2..4, 'a'), (6..9, 'b')]).unwrap();

    for range in [4..6, 7..7] {
        assert_eq!(marks(&layer, range.clone()), [], "{range:?}");
    }
}

#[test]
fn a_publish_at_a_chunk_edge_leaves_no_empty_chunk() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..10, &[(2..4, 'a')]).unwrap();

    publish(&mut layer, &buffer, 0..5, &[(1..2, 'b')]).unwrap();
    publish(&mut layer, &buffer, 7..10, &[]).unwrap();

    assert_eq!(chunks(&layer), [0..5, 5..7, 7..10]);
}

#[test]
fn an_overflow_loses_the_layer_until_it_catches_up() {
    let mut buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..10, &[(2..4, 'a')]).unwrap();
    assert!(!layer.is_empty());

    buffer.transact(|buffer| {
        for _ in 0..257 {
            buffer.insert(at(20), "x");
        }
    });

    assert_eq!(layer.catch_up(&buffer), Err(Lost));
    assert!(layer.is_empty());
    assert_eq!(layer.at(), buffer.edit_mark());
    assert_eq!(layer.catch_up(&buffer), Ok(()));
}

#[test]
fn a_splice_inside_a_chunk_grows_it() {
    let mut buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..10, &[(2..4, 'a'), (6..9, 'b')]).unwrap();
    publish(&mut layer, &buffer, 10..20, &[(12..15, 'c')]).unwrap();

    buffer.transact(|buffer| buffer.replace(span(7..8), "xyz"));
    layer.catch_up(&buffer).unwrap();

    assert_eq!(chunks(&layer), [0..12, 12..22]);
    assert_eq!(
        marks(&layer, 0..22),
        [(2..4, 'a'), (6..7, 'b'), (10..11, 'b'), (14..17, 'c')]
    );
}

#[test]
fn a_splice_reaching_a_chunk_edge_cuts_it_by_the_mark_rule() {
    for (removed, expected) in [(5..6, 8..17), (14..15, 5..14)] {
        let mut buffer = twenty();
        let mut layer = Layer::new(buffer.edit_mark());
        publish(&mut layer, &buffer, 5..15, &[]).unwrap();

        buffer.transact(|buffer| buffer.replace(span(removed.clone()), "xyz"));
        layer.catch_up(&buffer).unwrap();

        assert_eq!(chunks(&layer), [expected], "{removed:?}");
    }
}

#[test]
fn a_deletion_inside_a_mark_leaves_one_mark() {
    let mut buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..20, &[(2..8, 'a')]).unwrap();

    buffer.transact(|buffer| buffer.delete(span(4..6)));
    layer.catch_up(&buffer).unwrap();

    assert_eq!(marks(&layer, 0..18), [(2..6, 'a')]);
}

#[test]
fn a_publish_past_the_ring_is_lost() {
    let mut buffer = twenty();
    let old = buffer.edit_mark();
    buffer.transact(|buffer| {
        for _ in 0..257 {
            buffer.insert(at(20), "x");
        }
    });
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..10, &[(2..4, 'a')]).unwrap();

    let published = layer.publish(&buffer, span(0..10), vec![(span(6..8), 'b')], old);

    assert_eq!(published, Err(Lost));
    assert!(layer.is_empty());
}

#[test]
fn an_older_publish_beside_a_newer_chunk_lands() {
    let mut buffer = twenty();
    let old = buffer.edit_mark();
    buffer.transact(|buffer| buffer.insert(at(20), "x"));
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 10..21, &[(12..15, 'b')]).unwrap();

    let published = layer.publish(&buffer, span(0..10), vec![(span(2..4), 'a')], old);

    assert_eq!(published, Ok(true));
    assert_eq!(marks(&layer, 0..21), [(2..4, 'a'), (12..15, 'b')]);
}

#[test]
#[should_panic(expected = "past the end")]
fn a_chunk_past_the_end_panics() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());

    let _ = publish(&mut layer, &buffer, 0..25, &[]);
}
