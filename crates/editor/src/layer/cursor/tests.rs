//! A publish against the reference model: each change in turn, mapped by `map_chunk` over every
//! splice since, placed by the list-wide replace the cursor replaced; a late chunk an edit reached
//! does not land (code highlighting spec, S2b deviations 1 and 2).

use std::ops::Range;

use inkra_text::{Buffer, ByteOffset, EditMark, Splice};
use proptest::prelude::*;

use crate::layer::map::map_chunk;
use crate::layer::{Chunk, Layer, Lost, Published, differing};

#[derive(Debug, Clone)]
enum Step {
    /// A chunk's bounds, its marks' bounds and its tag, as picks among the char boundaries.
    Publish((usize, usize), Vec<usize>, u64),
    /// Replaces up to `len` chars from a pick with the text.
    Edit(usize, usize, String),
}

fn text_strategy() -> impl Strategy<Value = String> {
    prop_oneof!["[a-z]{1,3}", Just("é".to_string()), Just("🎉".to_string())]
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        (
            (any::<usize>(), any::<usize>()),
            prop::collection::vec(any::<usize>(), 0..6),
            0u64..3
        )
            .prop_map(|(bounds, marks, tag)| Step::Publish(bounds, marks, tag)),
        (any::<usize>(), 0usize..4, prop::option::of(text_strategy()))
            .prop_map(|(at, len, text)| Step::Edit(at, len, text.unwrap_or_default())),
    ]
}

fn boundaries(text: &str) -> Vec<usize> {
    (0..=text.len())
        .filter(|&index| text.is_char_boundary(index))
        .collect()
}

fn span(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}

/// The chunk `bounds` and `marks` pick in `text`, each mark styled by its rank.
fn computed(text: &str, (a, b): (usize, usize), picks: &[usize], tag: u64) -> Published<u8> {
    let all = boundaries(text);
    let (a, b) = (all[a % all.len()], all[b % all.len()]);
    let inside: Vec<_> = all
        .into_iter()
        .filter(|&i| a.min(b) <= i && i <= a.max(b))
        .collect();
    let mut ends: Vec<_> = picks
        .iter()
        .map(|pick| inside[pick % inside.len()])
        .collect();
    ends.sort_unstable();
    ends.dedup();
    let marks = (0..)
        .zip(ends.as_chunks().0)
        .map(|(style, &[start, end])| (span(start..end), style))
        .collect();
    (span(a.min(b)..a.max(b)), tag, marks)
}

fn edit(buffer: &mut Buffer, at: usize, len: usize, text: &str) {
    let all = boundaries(buffer.text());
    let start = all[at % all.len()];
    let end = all
        .iter()
        .copied()
        .filter(|&i| i >= start)
        .nth(len)
        .unwrap_or(buffer.text().len());
    buffer.transact(|buffer| buffer.replace(span(start..end), text));
}

/// Runs `steps` on both layers, each publish at the buffer's edit mark.
fn run(buffer: &mut Buffer, layers: &mut [&mut Layer<u8>; 2], steps: &[Step]) {
    for step in steps {
        match step {
            Step::Publish(bounds, picks, tag) => {
                let chunk = computed(buffer.text(), *bounds, picks, *tag);
                for layer in layers.iter_mut() {
                    let at = buffer.edit_mark();
                    let published = layer.publish(buffer, Vec::new(), vec![chunk.clone()], at);
                    assert!(published.is_ok(), "{published:?}");
                }
            }
            Step::Edit(at, len, text) => edit(buffer, *at, *len, text),
        }
    }
}

/// The reference publish: every change in turn, as `Layer::publish` did before the cursor.
fn reference(
    layer: &mut Layer<u8>,
    buffer: &Buffer,
    removed: Vec<Range<ByteOffset>>,
    chunks: Vec<Published<u8>>,
    at: EditMark,
) -> Result<Option<Range<ByteOffset>>, Lost> {
    layer.catch_up(buffer)?;
    let splices: Vec<Splice> = buffer.edits_since(at).ok_or(Lost)?.collect();
    let removed = removed
        .into_iter()
        .map(|range| (Chunk::new(range, at, None, []), false));
    let chunks = chunks
        .into_iter()
        .map(|(range, tag, marks)| (Chunk::new(range, at, Some(tag), marks), true));
    let mut span: Option<Range<ByteOffset>> = None;
    for (new, fills) in removed.chain(chunks) {
        let Some(new) = splices
            .iter()
            .copied()
            .try_fold(new, map_chunk)
            .filter(|new| new.len != 0)
        else {
            continue;
        };
        // `map_chunk` drops the tag of a chunk a splice reached.
        if fills && !splices.is_empty() && new.tag.is_none() {
            continue;
        }
        if let Some(range) = replace(layer, new, fills) {
            span = Some(span.map_or(range.clone(), |span| {
                span.start.min(range.start)..span.end.max(range.end)
            }));
        }
    }
    Ok(span)
}

/// The list-wide replace: drains what `new`'s range holds and splices the pieces back.
fn replace(layer: &mut Layer<u8>, new: Chunk<u8>, fills: bool) -> Option<Range<ByteOffset>> {
    let at = new.published;
    let chunk = new.start..new.end();
    let new = fills.then_some(new);
    let first = layer.chunks.partition_point(|old| old.end() <= chunk.start);
    let last = layer.chunks.partition_point(|old| old.start < chunk.end);
    let owned = |layer: &Layer<u8>| -> Vec<_> {
        layer
            .marks_in(chunk.clone())
            .map(|(mark, &style)| (mark, style))
            .collect()
    };
    let before = owned(layer);
    let mut pieces = Vec::new();
    let mut tail = None;
    let mut gap = chunk.start;
    for old in layer.chunks.drain(first..last) {
        if old.published > at {
            pieces.extend(new.as_ref().and_then(|new| new.piece(gap..old.start)));
            gap = old.end();
            pieces.push(old);
        } else {
            pieces.extend(old.piece(old.start..chunk.start));
            tail = old.piece(chunk.end..old.end());
        }
    }
    pieces.extend(new.as_ref().and_then(|new| new.piece(gap..chunk.end)));
    pieces.extend(tail);
    layer.chunks.splice(first..first, pieces);
    differing(&before, &owned(layer))
}

type Held = (
    Range<ByteOffset>,
    EditMark,
    Option<u64>,
    Vec<(Range<ByteOffset>, u8)>,
);

fn held(layer: &Layer<u8>) -> Vec<Held> {
    layer
        .chunks
        .iter()
        .map(|chunk| {
            let marks = chunk
                .marks
                .iter()
                .map(|mark| (mark.range(chunk.start), mark.style))
                .collect();
            (chunk.start..chunk.end(), chunk.published, chunk.tag, marks)
        })
        .collect()
}

proptest! {
    #[test]
    fn a_publish_equals_its_changes_one_by_one_with_reached_late_chunks_skipped(
        parts in prop::collection::vec(text_strategy(), 1..10),
        before in prop::collection::vec(step_strategy(), 0..8),
        removed in prop::collection::vec((any::<usize>(), any::<usize>()), 0..3),
        chunks in prop::collection::vec(
            ((any::<usize>(), any::<usize>()), prop::collection::vec(any::<usize>(), 0..6)),
            0..4,
        ),
        after in prop::collection::vec(step_strategy(), 0..8),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        let mut model = Layer::new(buffer.edit_mark());
        run(&mut buffer, &mut [&mut layer, &mut model], &before);
        let at = buffer.edit_mark();
        let text = buffer.text();
        let removed: Vec<_> = removed
            .into_iter()
            .map(|bounds| computed(text, bounds, &[], 0).0)
            .collect();
        let chunks: Vec<_> = (10..)
            .zip(&chunks)
            .map(|(tag, (bounds, picks))| computed(text, *bounds, picks, tag))
            .collect();
        run(&mut buffer, &mut [&mut layer, &mut model], &after);

        let expected = reference(&mut model, &buffer, removed.clone(), chunks.clone(), at);
        let published = layer.publish(&buffer, removed, chunks, at);

        prop_assert_eq!(published, expected);
        prop_assert_eq!(held(&layer), held(&model));
    }
}
