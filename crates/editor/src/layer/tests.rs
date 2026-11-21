use inkra_text::{Buffer, ByteOffset};

use super::*;

pub(super) fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

pub(super) fn span(range: Range<usize>) -> Range<ByteOffset> {
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
) -> Result<Option<Range<usize>>, Lost> {
    let marks = marks
        .iter()
        .map(|(range, style)| (span(range.clone()), *style))
        .collect();
    let published = layer.publish(
        buffer,
        Vec::new(),
        vec![(span(chunk), 0, marks)],
        buffer.edit_mark(),
    );
    published.map(|range| range.map(|range| range.start.get()..range.end.get()))
}

pub(super) fn twenty() -> Buffer {
    Buffer::new("abcdefghijklmnopqrst")
}

#[test]
fn a_publish_replaces_the_marks_inside_its_chunk_only() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    let first = [(2..4, 'a'), (6..9, 'b'), (12..15, 'c')];
    assert_eq!(publish(&mut layer, &buffer, 0..20, &first), Ok(Some(2..15)));

    assert_eq!(
        publish(&mut layer, &buffer, 5..10, &[(7..8, 'd')]),
        Ok(Some(6..9))
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
    assert_eq!(publish(&mut layer, &buffer, 0..10, &marks), Ok(Some(2..9)));

    assert_eq!(publish(&mut layer, &buffer, 0..10, &marks), Ok(None));
}

/// The syntax listener publishes an empty batch on every keystroke in prose.
#[test]
fn an_empty_publish_moves_no_chunk() {
    let mut buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 2..6, &[(3..4, 'a')]).unwrap();
    publish(&mut layer, &buffer, 8..12, &[]).unwrap();
    let old = buffer.edit_mark();
    buffer.transact(|buffer| buffer.insert(at(0), "x"));
    let held = layer.chunks.as_ptr();

    assert_eq!(
        layer.publish(&buffer, Vec::new(), Vec::new(), old),
        Ok(None)
    );

    assert_eq!(layer.chunks.as_ptr(), held);
    assert_eq!(chunks(&layer), [3..7, 9..13]);
    assert_eq!(layer.at(), buffer.edit_mark());
}

#[test]
fn a_publish_spans_only_the_marks_that_differ() {
    let first = [(1..3, 'a'), (5..7, 'b'), (9..11, 'c'), (13..15, 'd')];
    for (second, recoloured) in [
        (
            [(1..3, 'a'), (5..7, 'x'), (9..11, 'c'), (13..15, 'd')],
            5..7,
        ),
        (
            [(1..3, 'a'), (5..8, 'b'), (9..11, 'c'), (13..15, 'd')],
            5..8,
        ),
        (
            [(1..3, 'a'), (5..7, 'b'), (9..10, 'c'), (13..15, 'd')],
            9..11,
        ),
        (
            [(0..3, 'a'), (5..7, 'b'), (9..11, 'c'), (13..15, 'd')],
            0..3,
        ),
        (
            [(1..3, 'a'), (5..7, 'b'), (9..11, 'c'), (13..16, 'd')],
            13..16,
        ),
        (
            [(1..3, 'x'), (5..7, 'b'), (9..11, 'c'), (13..15, 'x')],
            1..15,
        ),
    ] {
        let buffer = twenty();
        let mut layer = Layer::new(buffer.edit_mark());
        publish(&mut layer, &buffer, 0..20, &first).unwrap();

        let published = publish(&mut layer, &buffer, 0..20, &second);

        assert_eq!(published, Ok(Some(recoloured)), "{second:?}");
    }
}

#[test]
fn a_mark_added_or_dropped_spans_itself() {
    let first = [(1..3, 'a'), (9..11, 'c')];
    for (second, recoloured) in [
        (&[(1..3, 'a'), (5..7, 'b'), (9..11, 'c')][..], 5..7),
        (&[(9..11, 'c')], 1..3),
        (&[(1..3, 'a')], 9..11),
    ] {
        let buffer = twenty();
        let mut layer = Layer::new(buffer.edit_mark());
        publish(&mut layer, &buffer, 0..20, &first).unwrap();

        let published = publish(&mut layer, &buffer, 0..20, second);

        assert_eq!(published, Ok(Some(recoloured)), "{second:?}");
    }
}

#[test]
fn a_batch_spans_every_change_it_made_and_none_it_did_not() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..20, &[(1..2, 'a'), (8..9, 'b')]).unwrap();
    let at = buffer.edit_mark();
    let chunk = |range: Range<usize>, marks: Vec<(Range<usize>, char)>| {
        let marks = marks.into_iter().map(|(mark, style)| (span(mark), style));
        (span(range), 0, marks.collect())
    };

    let unchanged = layer.publish(
        &buffer,
        vec![span(12..14)],
        vec![chunk(6..10, vec![(8..9, 'b')])],
        at,
    );
    let changed = layer.publish(
        &buffer,
        vec![span(0..3)],
        vec![chunk(15..20, vec![(16..17, 'c')])],
        at,
    );

    assert_eq!(unchanged, Ok(None));
    assert_eq!(changed, Ok(Some(span(1..17))));
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

    let published = layer.publish(
        &buffer,
        Vec::new(),
        vec![(span(0..10), 0, vec![(span(6..8), 'b')])],
        old,
    );

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

    let published = layer.publish(
        &buffer,
        Vec::new(),
        vec![(span(0..10), 0, vec![(span(2..4), 'a')])],
        old,
    );

    assert_eq!(published, Ok(Some(span(2..4))));
    assert_eq!(marks(&layer, 0..21), [(2..4, 'a'), (12..15, 'b')]);
}

#[test]
fn an_older_publish_over_a_newer_chunk_fills_around_it() {
    let mut buffer = twenty();
    let old = buffer.edit_mark();
    buffer.transact(|buffer| buffer.insert(at(20), "x"));
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 8..12, &[(9..10, 'n')]).unwrap();

    let marks_at_old = vec![(span(2..4), 'a'), (span(7..11), 'b'), (span(14..16), 'c')];
    let published = layer.publish(
        &buffer,
        Vec::new(),
        vec![(span(0..20), 0, marks_at_old)],
        old,
    );

    assert_eq!(published, Ok(Some(span(2..16))));
    assert_eq!(chunks(&layer), [0..8, 8..12, 12..20]);
    assert_eq!(
        marks(&layer, 0..21),
        [(2..4, 'a'), (7..8, 'b'), (9..10, 'n'), (14..16, 'c')]
    );
}

#[test]
#[should_panic(expected = "past the end")]
fn a_chunk_past_the_end_panics() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());

    let _ = publish(&mut layer, &buffer, 0..25, &[]);
}

#[test]
fn chunks_follow_edits() {
    let mut buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 2..6, &[(3..4, 'a')]).unwrap();
    publish(&mut layer, &buffer, 8..12, &[]).unwrap();
    assert_eq!(
        layer.chunks().collect::<Vec<_>>(),
        [(span(2..6), Some(0)), (span(8..12), Some(0))]
    );

    buffer.transact(|buffer| buffer.insert(at(0), "xyz"));
    layer.catch_up(&buffer).unwrap();

    assert_eq!(
        layer.chunks().collect::<Vec<_>>(),
        [(span(5..9), Some(0)), (span(11..15), Some(0))]
    );
}

#[test]
fn a_removal_leaves_no_chunk_inside_its_range_and_cuts_the_ones_it_reaches() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 0..6, &[(1..5, 'a')]).unwrap();
    publish(&mut layer, &buffer, 8..10, &[]).unwrap();
    publish(&mut layer, &buffer, 12..18, &[(13..17, 'b')]).unwrap();

    assert_eq!(
        layer.publish(&buffer, vec![span(4..14)], Vec::new(), buffer.edit_mark()),
        Ok(Some(span(4..14)))
    );

    assert_eq!(chunks(&layer), [0..4, 14..18]);
    assert_eq!(marks(&layer, 0..20), [(1..4, 'a'), (14..17, 'b')]);
}

#[test]
fn removing_a_chunk_without_marks_changes_no_mark() {
    let buffer = twenty();
    let mut layer = Layer::new(buffer.edit_mark());
    publish(&mut layer, &buffer, 8..10, &[]).unwrap();

    assert_eq!(
        layer.publish(&buffer, vec![span(8..10)], Vec::new(), buffer.edit_mark()),
        Ok(None)
    );

    assert!(layer.is_empty());
}

#[test]
fn a_late_removal_lands_where_its_range_moved_and_keeps_a_newer_chunk() {
    let mut buffer = twenty();
    let old = buffer.edit_mark();
    let mut layer = Layer::new(old);
    publish(&mut layer, &buffer, 2..6, &[(3..4, 'a')]).unwrap();
    publish(&mut layer, &buffer, 8..10, &[(8..9, 'b')]).unwrap();
    buffer.transact(|buffer| buffer.insert(at(0), "xyz"));
    publish(&mut layer, &buffer, 14..16, &[(14..15, 'n')]).unwrap();

    assert_eq!(
        layer.publish(&buffer, vec![span(8..16)], Vec::new(), old),
        Ok(Some(span(11..12)))
    );

    assert_eq!(chunks(&layer), [5..9, 14..16]);
    assert_eq!(marks(&layer, 0..23), [(6..7, 'a'), (14..15, 'n')]);
}

fn tags(layer: &Layer<char>) -> Vec<(Range<usize>, Option<u64>)> {
    layer
        .chunks()
        .map(|(range, tag)| (range.start.get()..range.end.get(), tag))
        .collect()
}

#[test]
fn a_stored_mark_takes_twelve_bytes() {
    assert!(size_of::<Mark<u8>>() <= 12);
}

pub(super) fn widest() -> usize {
    usize::try_from(u32::MAX).unwrap_or(usize::MAX)
}

pub(super) fn chunk_marks(chunk: &Chunk<char>) -> Vec<(Range<ByteOffset>, char)> {
    chunk
        .marks_in(at(0)..chunk.end())
        .map(|(mark, &style)| (mark, style))
        .collect()
}

#[test]
fn a_mark_ending_past_u32_from_its_chunk_start_drops_every_mark_and_the_tag() {
    for (end, kept) in [(widest(), true), (widest() + 1, false)] {
        let marks = [(span(1..2), 'a'), (span(end - 1..end), 'b')];
        let chunk = Chunk::new(
            at(0)..at(widest() + 1),
            EditMark::default(),
            Some(7),
            marks.clone(),
        );

        let marks = if kept { marks.to_vec() } else { vec![] };
        assert_eq!(chunk_marks(&chunk), marks, "{end}");
        assert_eq!(chunk.tag, kept.then_some(7), "{end}");
    }
}

#[test]
fn a_piece_cut_from_a_chunk_drops_its_tag() {
    let buffer = twenty();
    let mut layer = Layer::<char>::new(buffer.edit_mark());
    let at = buffer.edit_mark();
    layer
        .publish(&buffer, Vec::new(), vec![(span(0..10), 1, vec![])], at)
        .unwrap();

    layer
        .publish(&buffer, Vec::new(), vec![(span(5..20), 2, vec![])], at)
        .unwrap();

    assert_eq!(tags(&layer), [(0..5, None), (5..20, Some(2))]);
}
