use std::ops::Range;

use inkra_editor::Layer;
use inkra_text::{Buffer, ByteOffset, Splice};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Step {
    Publish(Chunk),
    /// Several splices, one catch-up.
    Edit(Vec<(usize, usize, String)>),
}

/// A chunk's two bounds and its marks' bounds, each a pick among the char boundaries.
#[derive(Debug, Clone)]
struct Chunk {
    bounds: (usize, usize),
    marks: Vec<(usize, u8)>,
}

fn text_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,3}",
        Just("é".to_string()),
        Just("日".to_string()),
        Just("🎉".to_string()),
        Just("e\u{301}".to_string()),
    ]
}

fn chunk_strategy() -> impl Strategy<Value = Chunk> {
    (
        (any::<usize>(), any::<usize>()),
        prop::collection::vec((any::<usize>(), 0u8..3), 0..8),
    )
        .prop_map(|(bounds, marks)| Chunk { bounds, marks })
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        chunk_strategy().prop_map(Step::Publish),
        prop::collection::vec(
            (any::<usize>(), 0usize..4, prop::option::of(text_strategy()))
                .prop_map(|(at, len, text)| (at, len, text.unwrap_or_default())),
            1..4,
        )
        .prop_map(Step::Edit),
    ]
}

/// The char boundary `pick` lands on among those in `range`, both ends included.
fn boundary(text: &str, range: Range<usize>, pick: usize) -> usize {
    let boundaries: Vec<usize> = (range.start..=range.end)
        .filter(|&index| text.is_char_boundary(index))
        .collect();
    boundaries[pick % boundaries.len()]
}

fn ordered(text: &str, range: Range<usize>, (a, b): (usize, usize)) -> Range<usize> {
    let (a, b) = (boundary(text, range.clone(), a), boundary(text, range, b));
    a.min(b)..a.max(b)
}

fn span(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}

/// Publishes `chunk` at the buffer's edit mark; returns its range.
fn publish(
    layer: &mut Layer<u8>,
    buffer: &Buffer,
    chunk: &Chunk,
) -> Result<Range<usize>, TestCaseError> {
    let text = buffer.text();
    let range = ordered(text, 0..text.len(), chunk.bounds);
    let mut bounds: Vec<usize> = chunk
        .marks
        .iter()
        .map(|&(pick, _)| boundary(text, range.clone(), pick))
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    let marks = bounds
        .as_chunks()
        .0
        .iter()
        .zip(&chunk.marks)
        .map(|(&[start, end], &(_, style))| (span(start..end), style))
        .collect();
    let published = layer.publish(buffer, span(range.clone()), marks, buffer.edit_mark());
    prop_assert!(published.is_ok(), "{:?}", published);
    Ok(range)
}

/// Replaces `len` chars from the boundary `at` picks inside `range` with `text`.
fn edit(buffer: &mut Buffer, range: Range<usize>, at: usize, len: usize, text: &str) {
    let start = boundary(buffer.text(), range.clone(), at);
    let end = (start..=range.end)
        .filter(|&index| buffer.text().is_char_boundary(index))
        .nth(len)
        .unwrap_or(range.end);
    buffer.transact(|buffer| buffer.replace(span(start..end), text));
}

fn marks(layer: &Layer<u8>, buffer: &Buffer) -> Vec<(Range<usize>, u8)> {
    layer
        .marks_in(span(0..buffer.text().len()))
        .map(|(range, &style)| (range.start.get()..range.end.get(), style))
        .collect()
}

/// Each byte's style, `None` where no mark covers it.
fn styles(layer: &Layer<u8>, buffer: &Buffer) -> Vec<Option<u8>> {
    let mut styles = vec![None; buffer.text().len()];
    for (range, style) in marks(layer, buffer) {
        styles[range].fill(Some(style));
    }
    styles
}

/// Each new byte's index in the text before `splices`, `None` for a byte they inserted.
fn provenance(splices: impl Iterator<Item = Splice>, old_len: usize) -> Vec<Option<usize>> {
    let mut provenance: Vec<_> = (0..old_len).map(Some).collect();
    for splice in splices {
        let at = splice.at.get();
        provenance.splice(
            at..at + splice.removed_len,
            std::iter::repeat_n(None, splice.inserted_len),
        );
    }
    provenance
}

proptest! {
    #[test]
    fn marks_stay_sorted_disjoint_and_on_char_boundaries(
        parts in prop::collection::vec(text_strategy(), 1..12),
        steps in prop::collection::vec(step_strategy(), 0..24),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        for step in steps {
            match step {
                Step::Publish(chunk) => {
                    publish(&mut layer, &buffer, &chunk)?;
                }
                Step::Edit(edits) => {
                    for (at, len, text) in edits {
                        let all = 0..buffer.text().len();
                        edit(&mut buffer, all, at, len, &text);
                    }
                    prop_assert_eq!(layer.catch_up(&buffer), Ok(()));
                }
            }
            let mut floor = 0;
            for (range, _) in marks(&layer, &buffer) {
                prop_assert!(floor <= range.start && range.start < range.end, "{:?} after {}", range, floor);
                for bound in [range.start, range.end] {
                    prop_assert!(buffer.text().is_char_boundary(bound), "{} splits a char", bound);
                }
                floor = range.end;
            }
        }
    }

    /// And every byte a mark kept stays marked in its style.
    #[test]
    fn marks_never_cover_a_new_byte(
        parts in prop::collection::vec(text_strategy(), 1..12),
        steps in prop::collection::vec(step_strategy(), 0..24),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        for step in steps {
            match step {
                Step::Publish(chunk) => {
                    publish(&mut layer, &buffer, &chunk)?;
                }
                Step::Edit(edits) => {
                    let before = styles(&layer, &buffer);
                    let mark = buffer.edit_mark();
                    for (at, len, text) in edits {
                        let all = 0..buffer.text().len();
                        edit(&mut buffer, all, at, len, &text);
                    }
                    prop_assert_eq!(layer.catch_up(&buffer), Ok(()));

                    let after = styles(&layer, &buffer);
                    let provenance = provenance(buffer.edits_since(mark).unwrap(), before.len());
                    for (byte, (style, old)) in after.iter().zip(provenance).enumerate() {
                        let expected = old.and_then(|old| before[old]);
                        prop_assert_eq!(*style, expected, "byte {}", byte);
                    }
                }
            }
        }
    }

    #[test]
    fn an_untouched_chunk_only_shifts(
        parts in prop::collection::vec(text_strategy(), 1..12),
        chunk in chunk_strategy(),
        before_it in any::<bool>(),
        at in any::<usize>(),
        len in 0usize..4,
        text in text_strategy(),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        let range = publish(&mut layer, &buffer, &chunk)?;
        let published = marks(&layer, &buffer);
        let old_len = buffer.text().len();

        let outside = if before_it { 0..range.start } else { range.end..old_len };
        edit(&mut buffer, outside, at, len, &text);
        prop_assert_eq!(layer.catch_up(&buffer), Ok(()));

        let shift = |offset: usize| {
            if before_it { offset + buffer.text().len() - old_len } else { offset }
        };
        let expected: Vec<_> = published
            .into_iter()
            .map(|(mark, style)| (shift(mark.start)..shift(mark.end), style))
            .collect();
        prop_assert_eq!(marks(&layer, &buffer), expected);
    }
}
