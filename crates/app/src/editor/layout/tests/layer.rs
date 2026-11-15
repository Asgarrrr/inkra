//! The oracle's decoration steps: a fake layer published, seeded, switched and lost (incremental
//! layout spec, D10).

use std::ops::Range;

use gpui::{App, Entity};
use inkra_text::{Buffer, ByteOffset, EditMark};
use proptest::prelude::*;
use proptest::sample::Index;

use super::floor;
use crate::decorations::{self, Chunk, LayerDecl, LayerId, MarkStyle};
use crate::feature::FeatureId;
use crate::theme::ColorToken;

const FEATURE: FeatureId = FeatureId::new("oracle_fake");

static LAYER: LayerDecl = LayerDecl {
    id: LayerId::new(FEATURE, "oracle"),
    open: None,
};

const COLORS: [Option<ColorToken>; 4] = [
    Some(ColorToken::Warning),
    Some(ColorToken::Accent),
    Some(ColorToken::SyntaxKeyword),
    None,
];

/// A chunk between two points, and marks between two points of it, each with a colour.
type ChunkSpec = (Index, Index, Vec<(Index, Index, usize)>);

#[derive(Debug, Clone)]
pub(super) enum Paint {
    /// A publish computed at an edit mark picked among those seen: ranges it clears, then chunks.
    Publish(Index, Vec<(Index, Index)>, Vec<ChunkSpec>),
    /// An open's chunks, landed at the buffer's edit mark.
    Seed(Vec<ChunkSpec>),
    /// The layer's feature switched on or off.
    Enable(bool),
    /// More splices than the buffer's log keeps, the text unchanged: a publish computed before is
    /// lost.
    Lose,
}

fn chunk_spec() -> impl Strategy<Value = ChunkSpec> {
    let marks = prop::collection::vec((any::<Index>(), any::<Index>(), 0..COLORS.len()), 0..4);
    (any::<Index>(), any::<Index>(), marks)
}

pub(super) fn paint() -> impl Strategy<Value = Paint> {
    let cleared = prop::collection::vec((any::<Index>(), any::<Index>()), 0..2);
    prop_oneof![
        4 => (any::<Index>(), cleared, prop::collection::vec(chunk_spec(), 0..3))
            .prop_map(|(at, cleared, chunks)| Paint::Publish(at, cleared, chunks)),
        1 => prop::collection::vec(chunk_spec(), 1..3).prop_map(Paint::Seed),
        1 => any::<bool>().prop_map(Paint::Enable),
        1 => Just(Paint::Lose),
    ]
}

/// Declares the layer alone and switches it on, with no entry left from an earlier case.
pub(super) fn declare(cx: &mut App) {
    decorations::declare([], cx);
    decorations::declare([&LAYER], cx);
    decorations::enable([FEATURE], cx);
}

/// Every edit mark seen, with the text at it, oldest first.
pub(super) struct Seen(Vec<(EditMark, String)>);

impl Seen {
    pub(super) fn new(buffer: &Buffer) -> Self {
        Self(vec![(buffer.edit_mark(), buffer.text().to_owned())])
    }

    pub(super) fn note(&mut self, buffer: &Buffer) {
        if self
            .0
            .last()
            .is_none_or(|(mark, _)| *mark != buffer.edit_mark())
        {
            self.0.push((buffer.edit_mark(), buffer.text().to_owned()));
        }
    }
}

/// From the earlier to the later of two points of `text`, on char boundaries, both at or past
/// `from`.
fn between(text: &str, from: usize, to: usize, a: Index, b: Index) -> Range<ByteOffset> {
    let [a, b] = [a, b].map(|at| floor(text, from + at.index(to - from + 1)));
    ByteOffset::new(a.min(b))..ByteOffset::new(a.max(b))
}

/// The chunk `spec` names in `text`, its marks sorted, any overlapping an earlier one dropped.
fn chunk(text: &str, (a, b, marks): &ChunkSpec) -> Chunk {
    let chunk = between(text, 0, text.len(), *a, *b);
    let (start, end) = (chunk.start.get(), chunk.end.get());
    let mut marks: Vec<_> = marks
        .iter()
        .map(|&(a, b, color)| {
            let style = MarkStyle {
                color: COLORS[color],
            };
            (between(text, start, end, a, b), style)
        })
        .collect();
    marks.sort_by_key(|(mark, _)| mark.start);
    let mut kept = chunk.start;
    marks.retain(|(mark, _)| {
        let keep = mark.start >= kept;
        if keep {
            kept = mark.end;
        }
        keep
    });
    (chunk, 0, marks)
}

pub(super) fn apply(paint: &Paint, buffer: &Entity<Buffer>, seen: &Seen, cx: &mut App) {
    match paint {
        Paint::Publish(at, cleared, chunks) => {
            let (mark, text) = &seen.0[at.index(seen.0.len())];
            let cleared = cleared
                .iter()
                .map(|&(a, b)| between(text, 0, text.len(), a, b))
                .collect();
            let chunks = chunks.iter().map(|spec| chunk(text, spec)).collect();
            decorations::publish(LAYER.id, &buffer.downgrade(), cleared, chunks, *mark, cx);
        }
        Paint::Seed(chunks) => {
            let text = buffer.read(cx).text();
            let chunks = chunks.iter().map(|spec| chunk(text, spec)).collect();
            decorations::seed(buffer, vec![(LAYER.id, chunks)], cx);
        }
        Paint::Enable(on) => decorations::enable(on.then_some(FEATURE), cx),
        Paint::Lose => buffer.update(cx, |buffer, _| {
            buffer.transact(|txn| {
                for _ in 0..129 {
                    txn.insert(ByteOffset::new(0), "a");
                    txn.delete(ByteOffset::new(0)..ByteOffset::new(1));
                }
            });
        }),
    }
}
