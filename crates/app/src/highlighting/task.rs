//! One background task per buffer lexes the blocks the main thread left pending, then lands them
//! (code highlighting spec, D10).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gpui::{App, AppContext as _, Entity, EntityId, Task, WeakEntity};
use inkra_highlight::{BlockChunk, backlog, lex_backlog};
use inkra_markdown::Markdown;
use inkra_text::{Buffer, EditMark};

use super::{SYNTAX, relex, styled};
use crate::decorations;

/// Each buffer's task in flight; the syntax listener owns it, so switching the feature off drops
/// every task, which cancels it.
#[derive(Default)]
pub(super) struct Flights {
    tasks: HashMap<EntityId, Task<()>>,
    /// The tasks spawned so far, for tests to count.
    #[cfg(test)]
    pub(super) spawned: usize,
}

/// Lexes a backlog of `pending` on the background executor and lands it, unless `buffer` has a
/// task in flight: that one replans when it lands.
pub(super) fn spawn(
    flights: &Rc<RefCell<Flights>>,
    buffer: &Entity<Buffer>,
    markdown: &Markdown,
    pending: &[usize],
    cx: &mut App,
) {
    let id = buffer.entity_id();
    if pending.is_empty() || flights.borrow().tasks.contains_key(&id) {
        return;
    }
    let text = buffer.read(cx);
    let (backlog, at) = (backlog(text.text(), markdown, pending), text.edit_mark());
    let lexing = cx.background_spawn(async move { lex_backlog(backlog) });
    let (weak, buffer) = (Rc::downgrade(flights), buffer.downgrade());
    let task = cx.spawn(async move |cx| {
        let chunks = lexing.await;
        cx.update(|cx| land(&weak, &buffer, chunks, at, cx));
    });
    let mut flights = flights.borrow_mut();
    flights.tasks.insert(id, task);
    #[cfg(test)]
    {
        flights.spawned += 1;
    }
}

/// D10's landing, in its order: `chunks` at `at`, or nothing when the log dropped the splices
/// since, then a replan of the current text that spawns the next task.
fn land(
    flights: &Weak<RefCell<Flights>>,
    buffer: &WeakEntity<Buffer>,
    chunks: Vec<BlockChunk>,
    at: EditMark,
    cx: &mut App,
) {
    let Some(flights) = flights.upgrade() else {
        return;
    };
    // Its own task: dropped after the borrow ends, and never seen by the replan below.
    let landing = flights.borrow_mut().tasks.remove(&buffer.entity_id());
    drop(landing);
    // Off, the publish lands nothing and the replan would respawn this task forever (S2c deviation 8).
    if !decorations::enabled(SYNTAX, cx) {
        return;
    }
    let Some(strong) = buffer.upgrade() else {
        return;
    };
    #[cfg(debug_assertions)]
    let carried = Carried::new(&chunks, at);
    let text = strong.read(cx);
    let (chunks, at) = if text.edits_since(at).is_some() {
        (chunks.into_iter().map(styled).collect(), at)
    } else {
        // At `at` the publish would be lost and clear the chunks the main thread published since.
        (Vec::new(), text.edit_mark())
    };
    decorations::publish(SYNTAX, buffer, Vec::new(), chunks, at, cx);
    #[cfg(debug_assertions)]
    carried.assert_held(&strong, cx);
    relex(&flights, &strong, None, cx);
}

/// What a landing carried, kept to check it landed exact.
#[cfg(debug_assertions)]
struct Carried {
    at: EditMark,
    blocks: Vec<(std::ops::Range<inkra_text::ByteOffset>, Option<u64>)>,
}

#[cfg(debug_assertions)]
impl Carried {
    fn new(chunks: &[BlockChunk], at: EditMark) -> Self {
        let blocks = chunks
            .iter()
            .map(|(range, shape, _)| (range.clone(), Some(*shape)))
            .collect();
        Self { at, blocks }
    }

    /// Else the replan sends the same blocks again after every landing (S2b deviation 4).
    fn assert_held(&self, buffer: &Entity<Buffer>, cx: &App) {
        if buffer.read(cx).edit_mark() != self.at {
            return;
        }
        let held = decorations::chunks(SYNTAX, buffer, cx);
        let exact = self.blocks.iter().all(|(range, tag)| {
            let at = held.partition_point(|(held, _)| held.start < range.start);
            held.get(at) == Some(&(range.clone(), *tag))
        });
        assert!(
            exact,
            "a landing at {:?} with no edit since left a block it carried inexact",
            self.at
        );
    }
}

#[cfg(test)]
mod tests;
