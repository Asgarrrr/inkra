//! How a chunk follows splices: its tag, `map_chunk`'s mark bound, and deviation 12's `follow`.

use std::ops::Range;

use inkra_text::{ByteOffset, EditMark, Splice};
use proptest::prelude::*;

use super::{follow, map_chunk};
use crate::layer::tests::{at, chunk_marks, span, twenty, widest};
use crate::layer::{Chunk, Layer};

#[test]
fn a_chunk_keeps_its_tag_until_a_splice_changes_its_bytes() {
    for late in [false, true] {
        for (removed, inserted, kept) in [
            (0..1, "", true),
            (18..19, "", true),
            (5..5, "x", true),
            (15..15, "x", true),
            (4..5, "", true),
            (15..16, "", true),
            (6..6, "x", false),
            (14..14, "x", false),
            (4..6, "", false),
            (14..16, "", false),
            (7..8, "y", false),
        ] {
            let mut buffer = twenty();
            let old = buffer.edit_mark();
            let mut layer = Layer::<char>::new(old);
            if !late {
                layer
                    .publish(&buffer, Vec::new(), vec![(span(5..15), 7, vec![])], old)
                    .unwrap();
            }

            buffer.transact(|buffer| buffer.replace(span(removed.clone()), inserted));
            if late {
                layer
                    .publish(&buffer, Vec::new(), vec![(span(5..15), 7, vec![])], old)
                    .unwrap();
            } else {
                layer.catch_up(&buffer).unwrap();
            }

            // A late chunk an edit reached does not land (code highlighting spec, S2b deviation 1).
            let tags: Vec<_> = layer.chunks().map(|(_, tag)| tag).collect();
            let expected = if late && !kept {
                vec![]
            } else {
                vec![kept.then_some(7)]
            };
            assert_eq!(tags, expected, "{late} {removed:?} {inserted:?}");
        }
    }
}

#[test]
fn a_splice_pushing_a_mark_past_u32_from_its_chunk_start_drops_every_mark() {
    for (inserted_len, kept) in [(1, true), (2, false)] {
        let last = widest() - 2..widest() - 1;
        let chunk = Chunk::new(
            at(0)..at(widest() - 1),
            EditMark::default(),
            None,
            [(span(1..2), 'a'), (span(last.clone()), 'b')],
        );
        let splice = Splice {
            at: at(10),
            removed_len: 0,
            inserted_len,
        };

        let grown =
            map_chunk(chunk, splice).unwrap_or_else(|| panic!("a splice inside keeps the chunk"));

        let marks = if kept {
            vec![(span(1..2), 'a'), (span(last.start + 1..last.end + 1), 'b')]
        } else {
            vec![]
        };
        assert_eq!(chunk_marks(&grown), marks, "{inserted_len}");
    }
}

/// A chunk's range, tag and marks once a fold put it in place.
type Followed = (
    Range<ByteOffset>,
    Option<u64>,
    Vec<(Range<ByteOffset>, char)>,
);

/// `None` when nothing of the chunk is left.
fn followed(chunk: Option<Chunk<char>>) -> Option<Followed> {
    chunk
        .filter(|chunk| chunk.len != 0)
        .map(|chunk| (chunk.start..chunk.end(), chunk.tag, chunk_marks(&chunk)))
}

proptest! {
    /// Deviation 12 of the code highlighting spec: a late publish skips the splices that miss it.
    #[test]
    fn following_splices_maps_a_chunk_as_map_chunk_over_every_splice(
        len in 0usize..48,
        bounds in (any::<usize>(), any::<usize>()),
        picks in prop::collection::vec(any::<usize>(), 0..8),
        tag in prop::option::of(0u64..3),
        edits in prop::collection::vec((any::<usize>(), 0usize..6, 0usize..6), 0..8),
    ) {
        let (a, b) = (bounds.0 % (len + 1), bounds.1 % (len + 1));
        let range = a.min(b)..a.max(b);
        let mut ends: Vec<_> = picks.iter().map(|pick| range.start + pick % (range.len() + 1)).collect();
        ends.sort_unstable();
        ends.dedup();
        let marks: Vec<_> = ends.as_chunks().0.iter().map(|&[start, end]| (span(start..end), 'a')).collect();
        let chunk = || Chunk::new(span(range.clone()), EditMark::default(), tag, marks.clone());
        let mut text_len = len;
        let splices: Vec<_> = edits
            .into_iter()
            .map(|(pick, removed, inserted_len)| {
                let start = pick % (text_len + 1);
                let removed_len = removed.min(text_len - start);
                text_len = text_len - removed_len + inserted_len;
                Splice { at: at(start), removed_len, inserted_len }
            })
            .collect();

        let skipping = splices.iter().copied().try_fold(chunk(), follow);
        let every = splices.iter().copied().try_fold(chunk(), map_chunk);

        prop_assert_eq!(followed(skipping), followed(every));
    }
}
