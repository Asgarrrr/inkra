use std::ops::Range;

use inkra_editor::{Layer, Lost};
use inkra_text::{Buffer, ByteOffset, EditMark, Splice};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Step {
    Publish(Chunk),
    /// Several splices, one catch-up.
    Edit(Vec<(usize, usize, String)>),
}

type Edits = Vec<(usize, usize, String)>;

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

fn edits_strategy() -> impl Strategy<Value = Edits> {
    prop::collection::vec(
        (any::<usize>(), 0usize..4, prop::option::of(text_strategy()))
            .prop_map(|(at, len, text)| (at, len, text.unwrap_or_default())),
        1..4,
    )
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        chunk_strategy().prop_map(Step::Publish),
        edits_strategy().prop_map(Step::Edit),
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

/// A publish computed against the text at `at`, delivered whenever.
#[derive(Clone)]
struct Publish {
    chunk: Range<usize>,
    marks: Vec<(Range<ByteOffset>, u8)>,
    at: EditMark,
}

impl Publish {
    /// `chunk`'s marks over `range` of `buffer`'s text now.
    fn new(buffer: &Buffer, range: Range<usize>, chunk: &Chunk) -> Self {
        let text = buffer.text();
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
        Self {
            chunk: range,
            marks,
            at: buffer.edit_mark(),
        }
    }

    /// Over the whole of `buffer`'s text now.
    fn whole(buffer: &Buffer, chunk: &Chunk) -> Self {
        Self::new(buffer, 0..buffer.text().len(), chunk)
    }

    /// Delivers the chunk, or with `removal`, empties its range.
    fn land(
        self,
        layer: &mut Layer<u8>,
        buffer: &Buffer,
        removal: bool,
    ) -> Result<Option<Range<ByteOffset>>, Lost> {
        if removal {
            layer.publish(buffer, vec![span(self.chunk)], Vec::new(), self.at)
        } else {
            self.deliver(layer, buffer)
        }
    }

    fn deliver(
        self,
        layer: &mut Layer<u8>,
        buffer: &Buffer,
    ) -> Result<Option<Range<ByteOffset>>, Lost> {
        let chunk = (span(self.chunk), 0, self.marks);
        layer.publish(buffer, Vec::new(), vec![chunk], self.at)
    }
}

/// Publishes `chunk` at the buffer's edit mark; returns its range.
fn publish(
    layer: &mut Layer<u8>,
    buffer: &Buffer,
    chunk: &Chunk,
) -> Result<Range<usize>, TestCaseError> {
    let text = buffer.text();
    let range = ordered(text, 0..text.len(), chunk.bounds);
    let published = Publish::new(buffer, range.clone(), chunk).deliver(layer, buffer);
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

/// Applies `edits` one after another, each anywhere in the text.
fn edit_all(buffer: &mut Buffer, edits: &Edits) {
    for (at, len, text) in edits {
        let all = 0..buffer.text().len();
        edit(buffer, all, *at, *len, text);
    }
}

fn tags(layer: &Layer<u8>) -> Vec<(Range<ByteOffset>, Option<u64>)> {
    layer.chunks().collect()
}

fn marks(layer: &Layer<u8>, buffer: &Buffer) -> Vec<(Range<usize>, u8)> {
    marks_in(layer, 0..buffer.text().len())
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

/// Runs `steps`, publishing at the buffer's edit mark and editing without catching up.
fn prepare(
    layer: &mut Layer<u8>,
    buffer: &mut Buffer,
    steps: Vec<Step>,
) -> Result<(), TestCaseError> {
    for step in steps {
        match step {
            Step::Publish(chunk) => {
                publish(layer, buffer, &chunk)?;
            }
            Step::Edit(edits) => edit_all(buffer, &edits),
        }
    }
    Ok(())
}

fn marks_in(layer: &Layer<u8>, range: Range<usize>) -> Vec<(Range<usize>, u8)> {
    layer
        .marks_in(span(range))
        .map(|(range, &style)| (range.start.get()..range.end.get(), style))
        .collect()
}

/// From the first to the last mark in one list and not the other, `None` when they hold the same.
fn differing(before: &[(Range<usize>, u8)], after: &[(Range<usize>, u8)]) -> Option<Range<usize>> {
    let only: Vec<_> = before
        .iter()
        .filter(|mark| !after.contains(mark))
        .chain(after.iter().filter(|mark| !before.contains(mark)))
        .map(|(mark, _)| mark.clone())
        .collect();
    let start = only.iter().map(|mark| mark.start).min()?;
    let end = only.iter().map(|mark| mark.end).max()?;
    Some(start..end)
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
                    edit_all(&mut buffer, &edits);
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

    #[test]
    fn chunks_are_sorted_disjoint_and_hold_every_mark(
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
                    edit_all(&mut buffer, &edits);
                    prop_assert_eq!(layer.catch_up(&buffer), Ok(()));
                }
            }
            let chunks: Vec<_> = layer.chunks().map(|(range, _)| range).collect();
            let mut floor = ByteOffset::new(0);
            for range in &chunks {
                prop_assert!(floor <= range.start && range.start < range.end, "{:?} after {}", range, floor);
                floor = range.end;
            }
            prop_assert!(floor <= buffer.len());
            for (mark, _) in layer.marks_in(span(0..buffer.text().len())) {
                let holders = chunks
                    .iter()
                    .filter(|range| range.start <= mark.start && mark.end <= range.end)
                    .count();
                prop_assert_eq!(holders, 1, "{:?}", mark);
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
                    edit_all(&mut buffer, &edits);
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

    #[test]
    fn a_late_publish_equals_an_early_publish_shifted(
        parts in prop::collection::vec(text_strategy(), 1..12),
        chunk in chunk_strategy(),
        edits in prop::collection::vec(edits_strategy(), 0..8),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut early = Layer::new(buffer.edit_mark());
        let mut late = Layer::new(buffer.edit_mark());
        let range = publish(&mut early, &buffer, &chunk)?;
        let computed = Publish::new(&buffer, range, &chunk);

        for edits in &edits {
            edit_all(&mut buffer, edits);
            prop_assert_eq!(early.catch_up(&buffer), Ok(()));
            prop_assert_eq!(late.catch_up(&buffer), Ok(()));
        }
        let published = computed.deliver(&mut late, &buffer);

        prop_assert!(published.is_ok(), "{:?}", published);
        prop_assert_eq!(marks(&late, &buffer), marks(&early, &buffer));
    }

    /// Byte by byte, the newer publish wins inside its chunk and the older one fills the rest.
    #[test]
    fn an_older_publish_fills_only_around_a_newer_chunk(
        parts in prop::collection::vec(text_strategy(), 1..12),
        older in chunk_strategy(),
        newer in chunk_strategy(),
        edits in prop::collection::vec(edits_strategy(), 1..8),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let text = buffer.text();
        let p1 = Publish::new(&buffer, ordered(text, 0..text.len(), older.bounds), &older);
        for edits in &edits {
            edit_all(&mut buffer, edits);
        }
        prop_assume!(p1.at < buffer.edit_mark());
        let mut older_alone = Layer::new(buffer.edit_mark());
        prop_assert!(p1.clone().deliver(&mut older_alone, &buffer).is_ok());
        let mut layer = Layer::new(buffer.edit_mark());
        let range = publish(&mut layer, &buffer, &newer)?;
        let newer_alone = styles(&layer, &buffer);

        prop_assert!(p1.deliver(&mut layer, &buffer).is_ok());

        let expected: Vec<_> = styles(&older_alone, &buffer)
            .into_iter()
            .zip(newer_alone)
            .enumerate()
            .map(|(byte, (older, newer))| if range.contains(&byte) { newer } else { older })
            .collect();
        prop_assert_eq!(styles(&layer, &buffer), expected);
    }

    #[test]
    fn out_of_order_publishes_keep_the_newer_marks(
        parts in prop::collection::vec(text_strategy(), 1..12),
        older in chunk_strategy(),
        newer in chunk_strategy(),
        edits in prop::collection::vec(edits_strategy(), 1..8),
        newer_first in any::<bool>(),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let p1 = Publish::whole(&buffer, &older);
        for edits in &edits {
            edit_all(&mut buffer, edits);
        }
        prop_assume!(p1.at < buffer.edit_mark());
        let p2 = Publish::whole(&buffer, &newer);
        let mut alone = Layer::new(buffer.edit_mark());
        prop_assert!(p2.clone().deliver(&mut alone, &buffer).is_ok());

        let mut layer = Layer::new(p1.at);
        let order = if newer_first { [p2, p1] } else { [p1, p2] };
        for computed in order {
            let published = computed.deliver(&mut layer, &buffer);
            prop_assert!(published.is_ok(), "{:?}", published);
        }

        prop_assert_eq!(marks(&layer, &buffer), marks(&alone, &buffer));
    }

    /// A removal or a chunk, computed at the buffer's edit mark or before edits.
    #[test]
    fn a_publish_spans_every_byte_it_recoloured(
        parts in prop::collection::vec(text_strategy(), 1..12),
        steps in prop::collection::vec(step_strategy(), 0..12),
        chunk in chunk_strategy(),
        removal in any::<bool>(),
        edits in prop::collection::vec(edits_strategy(), 0..3),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        prepare(&mut layer, &mut buffer, steps)?;
        let text = buffer.text();
        let computed = Publish::new(&buffer, ordered(text, 0..text.len(), chunk.bounds), &chunk);
        for edits in &edits {
            edit_all(&mut buffer, edits);
        }
        prop_assert_eq!(layer.catch_up(&buffer), Ok(()));
        let before = styles(&layer, &buffer);

        let published = computed.land(&mut layer, &buffer, removal);

        prop_assert!(published.is_ok(), "{:?}", published);
        let range = published.unwrap_or_default().map(|range| range.start.get()..range.end.get());
        let after = styles(&layer, &buffer);
        for (byte, (old, new)) in before.iter().zip(&after).enumerate() {
            let inside = range.as_ref().is_some_and(|range| range.contains(&byte));
            prop_assert!(old == new || inside, "byte {} outside {:?}", byte, range);
        }
    }

    /// Tight: the first and the last mark that differ in the chunk bound it, not the chunk.
    #[test]
    fn a_publish_spans_no_mark_it_left_alone(
        parts in prop::collection::vec(text_strategy(), 1..12),
        steps in prop::collection::vec(step_strategy(), 0..12),
        chunk in chunk_strategy(),
        removal in any::<bool>(),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut layer = Layer::new(buffer.edit_mark());
        prepare(&mut layer, &mut buffer, steps)?;
        prop_assert_eq!(layer.catch_up(&buffer), Ok(()));
        let text = buffer.text();
        let computed = Publish::new(&buffer, ordered(text, 0..text.len(), chunk.bounds), &chunk);
        let range = computed.chunk.clone();
        let before = marks_in(&layer, range.clone());

        let published = computed.land(&mut layer, &buffer, removal);

        let expected = differing(&before, &marks_in(&layer, range)).map(span);
        prop_assert_eq!(published, Ok(expected));
    }

    /// Removals first, then chunks, each list in its order, all computed at one edit mark.
    #[test]
    fn a_batch_equals_its_changes_one_at_a_time(
        parts in prop::collection::vec(text_strategy(), 1..12),
        steps in prop::collection::vec(step_strategy(), 0..12),
        removed in prop::collection::vec((any::<usize>(), any::<usize>()), 0..4),
        chunks in prop::collection::vec(chunk_strategy(), 0..4),
        edits in prop::collection::vec(edits_strategy(), 0..3),
    ) {
        let mut buffer = Buffer::new(parts.concat());
        let mut batched = Layer::new(buffer.edit_mark());
        let mut single = Layer::new(buffer.edit_mark());
        for step in steps {
            match step {
                Step::Publish(chunk) => {
                    publish(&mut batched, &buffer, &chunk)?;
                    publish(&mut single, &buffer, &chunk)?;
                }
                Step::Edit(edits) => edit_all(&mut buffer, &edits),
            }
        }
        let text = buffer.text();
        let removed: Vec<_> = removed
            .into_iter()
            .map(|bounds| span(ordered(text, 0..text.len(), bounds)))
            .collect();
        let chunks: Vec<_> = (1..)
            .zip(&chunks)
            .map(|(tag, chunk)| {
                let computed = Publish::new(&buffer, ordered(text, 0..text.len(), chunk.bounds), chunk);
                (span(computed.chunk), tag, computed.marks)
            })
            .collect();
        let at = buffer.edit_mark();
        for edits in &edits {
            edit_all(&mut buffer, edits);
        }

        let mut changed: Option<Range<ByteOffset>> = None;
        let removals = removed.iter().map(|range| (vec![range.clone()], Vec::new()));
        let fills = chunks.iter().map(|chunk| (Vec::new(), vec![chunk.clone()]));
        for (removed, chunks) in removals.chain(fills) {
            let published = single.publish(&buffer, removed, chunks, at);
            prop_assert!(published.is_ok(), "{:?}", published);
            if let Ok(Some(range)) = published {
                changed = Some(changed.map_or(range.clone(), |span| {
                    span.start.min(range.start)..span.end.max(range.end)
                }));
            }
        }
        prop_assert_eq!(single.catch_up(&buffer), Ok(()));
        let published = batched.publish(&buffer, removed, chunks, at);

        prop_assert_eq!(published, Ok(changed));
        prop_assert_eq!(tags(&batched), tags(&single));
        prop_assert_eq!(marks(&batched, &buffer), marks(&single, &buffer));
    }
}
