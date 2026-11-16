use std::cell::RefCell;
use std::rc::Weak;

use gpui::{App, Entity, Subscription, TestAppContext, prelude::*};
use inkra_highlight::{MAIN_THREAD_BUDGET, TASK_BYTES};
use inkra_text::{Buffer, Txn};

use super::Flights;
use crate::decorations;
use crate::editor::EditorView;
use crate::highlighting::tests::{at, blocks, fresh, kept, listen, marks, seed, settle, text};
use crate::highlighting::{FEATURE, MANIFEST};
use crate::theme::ColorToken;

/// A rust block of `lines` lines, `11 * lines + 11` bytes long, fences included.
fn rust_block(lines: usize) -> String {
    format!("```rust\n{}```", "let x = 1;\n".repeat(lines))
}

/// Over `MAIN_THREAD_BUDGET`: never lexed on the main thread.
fn large_block() -> String {
    let block = rust_block(MAIN_THREAD_BUDGET / 11);
    assert!(block.len() > MAIN_THREAD_BUDGET);
    block
}

/// The `marks` of `text` inside its blocks at `indices`.
fn inside(
    marks: Vec<(usize, usize, ColorToken)>,
    text: &str,
    indices: &[usize],
) -> Vec<(usize, usize, ColorToken)> {
    let blocks = blocks(text);
    marks
        .into_iter()
        .filter(|&(start, _, _)| indices.iter().any(|&index| blocks[index].contains(&start)))
        .collect()
}

/// The colours a whole lex of `text` gives inside its blocks at `indices`.
fn coloured(text: &str, indices: &[usize]) -> Vec<(usize, usize, ColorToken)> {
    inside(fresh(text), text, indices)
}

/// Declares the syntax layer, switches it on and listens, as the feature's activation does.
fn listening(cx: &mut TestAppContext) -> (Subscription, Weak<RefCell<Flights>>) {
    cx.update(|cx| {
        decorations::declare(MANIFEST.layers, cx);
        decorations::enable([FEATURE], cx);
        listen(cx)
    })
}

/// `text` in a buffer heard once, as a frame hears it, with `before` run first; nothing in flight
/// has run yet.
fn heard(
    cx: &mut TestAppContext,
    text: &str,
    before: impl FnOnce(&Entity<Buffer>, &mut App),
) -> (Entity<Buffer>, Subscription, Weak<RefCell<Flights>>) {
    let (listening, flights) = listening(cx);
    let buffer = cx.new(|_| Buffer::new(text));
    cx.update(|cx| before(&buffer, cx));
    hear(cx, &buffer);
    (buffer, listening, flights)
}

/// What a frame does before it draws `buffer`, with no task run.
fn hear(cx: &mut TestAppContext, buffer: &Entity<Buffer>) {
    cx.update(|cx| decorations::sync(buffer, cx));
}

fn type_in(cx: &mut TestAppContext, buffer: &Entity<Buffer>, change: impl FnOnce(&mut Txn<'_>)) {
    buffer.update(cx, |buffer, _| buffer.transact(change));
}

fn spawned(flights: &Weak<RefCell<Flights>>) -> usize {
    flights
        .upgrade()
        .map_or(0, |flights| flights.borrow().spawned)
}

fn flying(flights: &Weak<RefCell<Flights>>, buffer: &Entity<Buffer>) -> bool {
    flights
        .upgrade()
        .is_some_and(|flights| flights.borrow().tasks.contains_key(&buffer.entity_id()))
}

/// Runs the executor until `buffer`'s task in flight has landed, and no further.
fn land(cx: &mut TestAppContext, flights: &Weak<RefCell<Flights>>, buffer: &Entity<Buffer>) {
    assert!(flying(flights, buffer), "no task in flight");
    let before = spawned(flights);
    while flying(flights, buffer) && spawned(flights) == before {
        assert!(cx.executor().tick(), "the task never landed");
    }
}

/// Three blocks of 27,511 bytes: two fit in the budget, the third does not.
fn three_blocks() -> String {
    let block = rust_block(2_500);
    assert!(2 * block.len() <= MAIN_THREAD_BUDGET && 3 * block.len() > MAIN_THREAD_BUDGET);
    format!("{block}\n\n{block}\n\n{block}\n")
}

/// Not fresh, so the whole text is dirty.
#[gpui::test]
fn a_first_hearing_lexes_within_the_budget_and_the_task_lands_the_rest(cx: &mut TestAppContext) {
    let note = three_blocks();
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    assert_eq!(marks(cx, &buffer), coloured(&note, &[0, 1]));
    assert_eq!(spawned(&flights), 1);

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&note));
    assert!(!flying(&flights, &buffer));
}

#[gpui::test]
fn an_edit_in_a_block_lexes_it_on_the_main_thread(cx: &mut TestAppContext) {
    let note = "```rust\nlet\n```\n\n```rust\nmut\n```\n";
    let (buffer, _listening, flights) = heard(cx, note, |_, _| {});
    assert_eq!(marks(cx, &buffer), fresh(note));

    type_in(cx, &buffer, |t| t.insert(at(25), "fn "));
    hear(cx, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
    assert_eq!(spawned(&flights), 0);
}

#[gpui::test]
fn a_fresh_open_leaves_the_blocks_it_did_not_reach_to_the_task(cx: &mut TestAppContext) {
    let note = "```rust\nlet\n```\n\n```rust\nmut\n```\n";
    let (buffer, _listening, flights) = heard(cx, note, |buffer, cx| seed(buffer, 1, cx));

    assert_eq!(marks(cx, &buffer), inside(kept(note, &[]), note, &[0]));
    assert_eq!(spawned(&flights), 1);

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), kept(note, &[1]));
}

#[gpui::test]
fn a_block_over_the_budget_keeps_its_old_marks_until_the_task_lands(cx: &mut TestAppContext) {
    let note = format!("{}\n", large_block());
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});
    assert_eq!(marks(cx, &buffer), []);
    land(cx, &flights, &buffer);
    let before = marks(cx, &buffer);
    assert_eq!(before, fresh(&note));

    // Between `let ` and `x`, where no mark starts or ends.
    type_in(cx, &buffer, |t| t.insert(at(12), "fn "));
    hear(cx, &buffer);

    let shifted: Vec<_> = before
        .into_iter()
        .map(|(start, end, token)| {
            let by = if start >= 12 { 3 } else { 0 };
            (start + by, end + by, token)
        })
        .collect();
    assert_eq!(marks(cx, &buffer), shifted);
    assert_eq!(spawned(&flights), 2);

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

/// No view hears the buffer again: only each landing's replan can spawn the next task.
#[gpui::test]
fn pending_blocks_over_task_bytes_colour_with_no_edit(cx: &mut TestAppContext) {
    let block = format!("{}\n\n", rust_block(400));
    let note = block.repeat((MAIN_THREAD_BUDGET + TASK_BYTES) / block.len() + 2);
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    settle(cx);

    assert_eq!(marks(cx, &buffer), fresh(&note));
    assert_eq!(spawned(&flights), 2);
}

#[gpui::test]
fn edits_during_a_flight_start_one_more_task_when_it_lands(cx: &mut TestAppContext) {
    let note = format!("{}\n", large_block());
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    for _ in 0..2 {
        type_in(cx, &buffer, |t| t.insert(at(12), "fn "));
        hear(cx, &buffer);
    }
    assert_eq!(spawned(&flights), 1);

    land(cx, &flights, &buffer);

    assert_eq!(spawned(&flights), 2);
    // The edits reached the block: its chunk landed nothing.
    assert_eq!(marks(cx, &buffer), []);

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
    assert!(!flying(&flights, &buffer));
}

/// One more than the splice log keeps.
const PAST_THE_LOG: usize = 257;

#[gpui::test]
fn a_landing_past_the_log_keeps_what_the_main_thread_published_since(cx: &mut TestAppContext) {
    let large = large_block();
    let note = format!("{large}\n\n```rust\nlet\n```\n");
    let small = large.len() + "\n\n```rust\n".len();
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    for _ in 0..PAST_THE_LOG {
        type_in(cx, &buffer, |t| t.insert(at(small), "1 "));
        hear(cx, &buffer);
    }
    let text = text(cx, &buffer);
    assert_eq!(marks(cx, &buffer), coloured(&text, &[1]));

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), coloured(&text, &[1]));
    assert_eq!(spawned(&flights), 2);

    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&text));
}

/// No frame heard the edits, so the layer is lost too when the task lands.
#[gpui::test]
fn a_buffer_lost_during_a_flight_colours_once_heard(cx: &mut TestAppContext) {
    let note = three_blocks();
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    for _ in 0..PAST_THE_LOG {
        type_in(cx, &buffer, |t| t.insert(at(12), "fn "));
    }
    land(cx, &flights, &buffer);
    hear(cx, &buffer);
    settle(cx);

    assert_eq!(marks(cx, &buffer), fresh(&text(cx, &buffer)));
}

/// The reload's splices lie above the block: its chunk lands, mapped through them.
#[gpui::test]
fn a_reload_during_a_flight_lands_the_chunk_where_the_block_went(cx: &mut TestAppContext) {
    let block = large_block();
    let note = format!("prose\n\n{block}\n");
    let (buffer, _listening, flights) = heard(cx, &note, |_, _| {});

    let reloaded = format!("other prose, longer\n\n{block}\n");
    buffer.update(cx, |buffer, _| {
        let diff = inkra_text::diff(buffer.version(), buffer.text(), &reloaded);
        assert!(buffer.apply_diff(&diff));
    });
    land(cx, &flights, &buffer);

    assert_eq!(marks(cx, &buffer), fresh(&reloaded));
    assert_eq!(spawned(&flights), 1);
    assert!(!flying(&flights, &buffer));
}

#[gpui::test]
fn switching_the_feature_off_during_a_flight_leaves_no_mark(cx: &mut TestAppContext) {
    let (buffer, listening, flights) = heard(cx, &format!("{}\n", large_block()), |_, _| {});

    cx.update(|cx| decorations::enable([], cx));
    drop(listening);
    settle(cx);

    assert!(flights.upgrade().is_none(), "the task outlived its feature");
    assert_eq!(marks(cx, &buffer), []);
}

/// Its landing publishes nothing, so a replan would find the same blocks pending, forever.
#[gpui::test]
fn a_layer_switched_off_under_its_live_listener_stops_at_the_landing(cx: &mut TestAppContext) {
    let (buffer, _listening, flights) = heard(cx, &three_blocks(), |_, _| {});

    cx.update(|cx| decorations::enable([], cx));
    settle(cx);

    assert!(!flying(&flights, &buffer));
    assert_eq!(spawned(&flights), 1);
    assert_eq!(marks(cx, &buffer), []);
}

#[gpui::test]
fn switching_the_feature_off_and_on_during_a_flight_runs_one_new_task(cx: &mut TestAppContext) {
    let note = three_blocks();
    let (buffer, listening, old) = heard(cx, &note, |_, _| {});

    cx.update(|cx| decorations::enable([], cx));
    drop(listening);
    let (_listening, flights) = listening_again(cx);
    hear(cx, &buffer);

    assert!(old.upgrade().is_none(), "the old task outlived its feature");
    assert_eq!(spawned(&flights), 1);
    settle(cx);
    assert_eq!(marks(cx, &buffer), fresh(&note));
    assert_eq!(spawned(&flights), 1);
}

fn listening_again(cx: &mut TestAppContext) -> (Subscription, Weak<RefCell<Flights>>) {
    cx.update(|cx| {
        decorations::enable([FEATURE], cx);
        listen(cx)
    })
}

#[gpui::test]
fn a_note_closed_during_a_flight_lands_into_nothing(cx: &mut TestAppContext) {
    let note = three_blocks();
    let (closed, _listening, flights) = heard(cx, &note, |_, _| {});
    drop(closed);
    cx.update(|_| {});

    let opened = cx.new(|_| Buffer::new(note.as_str()));
    hear(cx, &opened);
    assert_eq!(spawned(&flights), 2);
    settle(cx);

    assert_eq!(marks(cx, &opened), fresh(&note));
    let flights = flights.upgrade().expect("the feature is on");
    assert!(
        flights.borrow().tasks.is_empty(),
        "a closed note's task stays"
    );
    assert_eq!(cx.update(|cx| decorations::tracked(cx)), 1);
}

#[gpui::test]
fn two_buffers_in_flight_each_land_their_own_task(cx: &mut TestAppContext) {
    let (one, _listening, flights) = heard(cx, &three_blocks(), |_, _| {});
    let other = format!("prose\n\n{}", three_blocks());
    let two = cx.new(|_| Buffer::new(other.as_str()));
    hear(cx, &two);
    assert_eq!(spawned(&flights), 2);

    settle(cx);

    assert_eq!(marks(cx, &one), fresh(&three_blocks()));
    assert_eq!(marks(cx, &two), fresh(&other));
}

#[gpui::test]
fn the_task_lands_after_its_editor_is_dropped(cx: &mut TestAppContext) {
    let note = three_blocks();
    let (_listening, flights) = listening(cx);
    let buffer = cx.new(|_| Buffer::new(note.as_str()));
    let view = buffer.clone();
    let window = cx.add_window(|_, cx| EditorView::new(view, cx));
    assert_eq!(spawned(&flights), 1);

    window
        .update(cx, |_, window, _| window.remove_window())
        .expect("the window is open");
    settle(cx);

    assert_eq!(marks(cx, &buffer), fresh(&note));
}
