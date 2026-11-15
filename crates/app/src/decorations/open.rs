//! Open functions: a layer's marks computed off the main thread, on the text a note's buffer is
//! built from, and landed at its first edit mark (decoration layer spec, D8).

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{App, BorrowAppContext, Entity};
use inkra_text::{Buffer, ByteOffset};

use super::listen::timed;
use super::{Decorations, LayerId, MarkStyle, entry, rank};

/// A chunk's range, tag and the marks inside it, as `publish` takes them.
pub(crate) type Chunk = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, MarkStyle)>);

/// Runs off the main thread; returns each chunk it finished before the deadline.
pub(crate) type OpenFn = fn(&str, Instant) -> Vec<Chunk>;

/// The highlighting spec's landing budget (its D8), shared by every open function of a note.
const LANDING: Duration = Duration::from_millis(8);

/// Each enabled layer's open function, by rank.
pub(crate) fn openers(cx: &App) -> Vec<(LayerId, OpenFn)> {
    cx.try_global::<Decorations>()
        .map_or_else(Vec::new, |decorations| {
            decorations
                .declared
                .iter()
                .filter_map(|declared| {
                    let open = declared.decl.open.filter(|_| declared.enabled)?;
                    Some((declared.decl.id, open))
                })
                .collect()
        })
}

/// Runs `openers` in order on `text`, each timed, all within one deadline from `ready`.
pub(crate) fn run_openers(
    text: &str,
    openers: &[(LayerId, OpenFn)],
    ready: Instant,
) -> Vec<(LayerId, Vec<Chunk>)> {
    let deadline = ready + LANDING;
    openers
        .iter()
        .map(|&(layer, open)| (layer, timed(layer, || open(text, deadline))))
        .collect()
}

/// Lands `opened` on `buffer` at its edit mark, which its listener's first hearing compares.
///
/// # Panics
/// As `HeardLayer::land`.
pub(crate) fn seed(buffer: &Entity<Buffer>, opened: Vec<(LayerId, Vec<Chunk>)>, cx: &mut App) {
    // No layer, no entry: a buffer no feature colours stays untracked.
    if opened.is_empty() {
        return;
    }
    let opened: Vec<_> = opened
        .into_iter()
        .map(|(layer, opened)| (rank(layer, cx), opened))
        .collect();
    cx.update_global(|decorations: &mut Decorations, cx| {
        let entry = entry(
            &mut decorations.entries,
            decorations.declared.len(),
            buffer,
            cx,
        );
        let text = buffer.read(cx);
        let mut landed = false;
        // A rank switched off since its opener was read.
        let enabled = opened
            .into_iter()
            .filter(|&(rank, _)| decorations.declared[rank].enabled);
        for (rank, chunks) in enabled {
            landed |= entry.layers[rank].land(text, chunks);
        }
        if landed {
            entry.bump(&mut decorations.revisions);
        }
    });
}
