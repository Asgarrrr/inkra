//! Open functions: a layer's marks computed off the main thread, on the text a note's buffer is
//! built from, and landed at its first edit mark (decoration layer spec, D8).

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{App, BorrowAppContext, Entity};
use inkra_text::{Buffer, ByteOffset};

use super::listen::{Seed, timed};
use super::{Decorations, LayerId, MarkStyle, entry, rank};

/// A chunk's range, tag and the marks inside it, as `publish` takes them.
pub(crate) type Chunk = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, MarkStyle)>);

/// What an open function did before its deadline.
pub(crate) struct Opened {
    /// Each chunk it finished.
    pub(crate) chunks: Vec<Chunk>,
    /// What its listener takes up with `TextChange::Opened`.
    pub(crate) seed: Option<Seed>,
}

/// Runs off the main thread; stops early when the deadline passes.
pub(crate) type OpenFn = fn(&str, Instant) -> Opened;

/// The highlighting spec's landing budget (its D8), shared by every open function of a note.
const LANDING: Duration = Duration::from_millis(8);

/// Each enabled layer's open function, by rank.
pub(crate) fn openers(cx: &App) -> Vec<(LayerId, OpenFn)> {
    cx.try_global::<Decorations>()
        .map_or_else(Vec::new, |decorations| {
            decorations
                .declared
                .iter()
                .zip(&decorations.enabled)
                .filter_map(|(decl, &enabled)| Some((decl.id, decl.open.filter(|_| enabled)?)))
                .collect()
        })
}

/// Runs `openers` in order on `text`, each timed, all within one deadline from `ready`.
pub(crate) fn run_openers(
    text: &str,
    openers: &[(LayerId, OpenFn)],
    ready: Instant,
) -> Vec<(LayerId, Opened)> {
    let deadline = ready + LANDING;
    openers
        .iter()
        .map(|&(layer, open)| (layer, timed(layer, || open(text, deadline))))
        .collect()
}

/// Lands `opened` on `buffer` at its edit mark; each seed waits there for its listener.
///
/// # Panics
/// As `publish`.
pub(crate) fn seed(buffer: &Entity<Buffer>, opened: Vec<(LayerId, Opened)>, cx: &mut App) {
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
        let at = text.edit_mark();
        let mut landed = false;
        // A rank switched off since its opener was read.
        let enabled = opened
            .into_iter()
            .filter(|&(rank, _)| decorations.enabled[rank]);
        for (rank, Opened { chunks, seed }) in enabled {
            landed |= entry.layers[rank].publish(text, Vec::new(), chunks, at) == Ok(true);
            entry.seeds[rank] = seed.map(|seed| (at, seed));
        }
        if landed {
            decorations.revisions += 1;
            entry.revision = decorations.revisions;
        }
    });
}
