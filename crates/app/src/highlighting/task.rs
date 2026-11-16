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
    let Some(strong) = buffer.upgrade() else {
        return;
    };
    let text = strong.read(cx);
    let (chunks, at) = if text.edits_since(at).is_some() {
        (chunks.into_iter().map(styled).collect(), at)
    } else {
        // At `at` the publish would be lost and clear the chunks the main thread published since.
        (Vec::new(), text.edit_mark())
    };
    decorations::publish(SYNTAX, buffer, Vec::new(), chunks, at, cx);
    relex(&flights, &strong, None, cx);
}

#[cfg(test)]
mod tests;
