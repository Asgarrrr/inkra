//! A code block scrolls sideways on its own (code block scroll spec, S1). Headless, a code glyph
//! is 9.6 px, so a 120-glyph line is 1152 px against a 756 px column: `max` is 412.

use std::time::Instant;

use gpui::{
    Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase, VisualTestContext,
    WindowHandle, point, px,
};

use super::{assert_x, at, close, dispatching, tall_document};
use crate::editor::EditorView;
use crate::editor::block_scroll::GESTURE_GAP;
use crate::editor::layout::{line_shape_count, painted_clips, shape_count};
use crate::text_input::{InputHost, painted_caret};

fn long() -> String {
    "x".repeat(120)
}

/// One long block, then a body line.
pub(super) fn block() -> String {
    format!("```\n{}\n```\nbody", long())
}

/// Where block `ix` was drawn last, read again before each event.
fn center(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> Point<Pixels> {
    window
        .update(cx, |view, _, _| view.block_center(ix))
        .unwrap()
        .unwrap()
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>, delta: ScrollDelta, phase: TouchPhase) {
    cx.simulate_event(ScrollEvent {
        position: at,
        delta,
        touch_phase: phase,
        ..ScrollEvent::default()
    });
    cx.run_until_parked();
}

fn swipe(cx: &mut VisualTestContext, at: Point<Pixels>, delta: Point<Pixels>) {
    wheel(cx, at, ScrollDelta::Pixels(delta), TouchPhase::Started);
}

pub(super) fn swipe_block(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    ix: usize,
    x: f32,
) {
    let at = center(window, cx, ix);
    swipe(cx, at, point(px(x), px(0.)));
}

/// Block `ix`'s scroll, then the page's; a sum of advances is not exact, so within `assert_x`.
fn assert_scrolls(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    ix: usize,
    (block, page): (f32, f32),
) {
    let (x, y) = window
        .update(cx, |view, _, _| (view.block_x(ix), view.scroll_y()))
        .unwrap();
    assert_x(x, block);
    assert_x(y, page);
}

fn x_at(window: WindowHandle<EditorView>, cx: &mut VisualTestContext, offset: usize) -> Pixels {
    window
        .update(cx, |view, _, _| {
            view.input.last_layout().unwrap().x_for_offset(at(offset))
        })
        .unwrap()
}

#[gpui::test]
fn a_long_block_line_is_one_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_count(), 4);
            assert_x(layout.x_for_offset(at(124)), 1152.);
        })
        .unwrap();
}

#[gpui::test]
fn a_swipe_moves_only_its_block(cx: &mut TestAppContext) {
    let long = long();
    let text = format!("```\n{long}\n```\n```\n{long}\n```\nprose");
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    let offsets = [14, 143, 260];
    let before = offsets.map(|offset| x_at(window, &mut cx, offset));

    swipe_block(window, &mut cx, 0, -100.);

    assert_scrolls(window, &mut cx, 0, (100., 0.));
    assert_scrolls(window, &mut cx, 1, (0., 0.));
    let after = offsets.map(|offset| x_at(window, &mut cx, offset));
    assert_x(after[0], f32::from(before[0]) - 100.);
    assert_eq!(after[1..], before[1..]);
    close(&mut cx);
}

#[gpui::test]
fn a_swipe_clamps_and_keeps_the_gesture_at_the_limit(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    swipe_block(window, &mut cx, 0, -1000.);
    assert_scrolls(window, &mut cx, 0, (412., 0.));
    swipe_block(window, &mut cx, 0, -50.);
    assert_scrolls(window, &mut cx, 0, (412., 0.));
    swipe_block(window, &mut cx, 0, 2000.);
    assert_scrolls(window, &mut cx, 0, (0., 0.));
    close(&mut cx);
}

#[gpui::test]
fn a_vertical_delta_over_a_block_scrolls_the_page(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let at = center(window, &mut cx, 0);
    swipe(&mut cx, at, point(px(0.), px(-30.)));
    assert_scrolls(window, &mut cx, 0, (0., -30.));
    // Any vertical part makes the delta the page's, sideways part and all.
    let at = center(window, &mut cx, 0);
    swipe(&mut cx, at, point(px(-20.), px(-30.)));
    assert_scrolls(window, &mut cx, 0, (0., -60.));
}

#[gpui::test]
fn a_sideways_delta_over_a_block_that_fits_scrolls_nothing(cx: &mut TestAppContext) {
    let text = format!("```\nshort\n```\n{}", tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    swipe_block(window, &mut cx, 0, -30.);
    assert_scrolls(window, &mut cx, 0, (0., 0.));
}

/// A point on the prose row under block `ix`, read again before each event.
fn under(window: WindowHandle<EditorView>, cx: &mut VisualTestContext, ix: usize) -> Point<Pixels> {
    window
        .update(cx, |view, _, _| {
            let bounds = view.input.last_bounds().unwrap();
            let background = view.input.last_layout().unwrap().block_backgrounds()[ix];
            bounds.origin + point(background.center().x, background.bottom() + px(14.))
        })
        .unwrap()
}

fn pixels(x: f32, y: f32) -> ScrollDelta {
    ScrollDelta::Pixels(point(px(x), px(y)))
}

#[gpui::test]
fn a_vertical_gesture_crossing_a_block_stays_vertical(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let at = under(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -30.), TouchPhase::Started);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-4., 0.), TouchPhase::Moved);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -5.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (0., -35.));
}

#[gpui::test]
fn jitter_in_a_sideways_gesture_never_reaches_the_page(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-20., 0.), TouchPhase::Started);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -3.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (20., 0.));
    close(&mut cx);
}

#[gpui::test]
fn a_sideways_swipe_over_prose_leaves_the_page(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let at = under(window, &mut cx, 0);
    swipe(&mut cx, at, point(px(-30.), px(0.)));
    let offset = window
        .update(&mut cx, |view, _, _| view.viewport.handle().offset())
        .unwrap();
    assert_eq!(offset, Point::default());
}

#[gpui::test]
fn a_block_scrolled_into_view_takes_the_wheel(cx: &mut TestAppContext) {
    let text = format!("{}{}", tall_document(), block());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let page = window
        .update(&mut cx, |view, _, _| f32::from(view.scroll_y()))
        .unwrap();
    assert!(page < 0., "the page did not scroll");

    swipe_block(window, &mut cx, 0, -40.);
    assert_scrolls(window, &mut cx, 0, (40., page));
    close(&mut cx);
}

#[gpui::test]
fn a_lines_delta_scrolls_a_block(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();

    let at = center(window, &mut cx, 0);
    wheel(
        &mut cx,
        at,
        ScrollDelta::Lines(point(-2., 0.)),
        TouchPhase::Started,
    );
    assert_scrolls(window, &mut cx, 0, (56., 0.));
    close(&mut cx);
}

#[gpui::test]
fn a_keystroke_reshape_keeps_the_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -96.);

    window
        .update(&mut cx, |view, window, cx| {
            let before = view.input.last_layout().cloned().unwrap();
            view.replace_text_in_range(Some(133..133), "s", window, cx);
            let layout = view.input.last_layout().unwrap();
            assert!(!std::rc::Rc::ptr_eq(&before, layout), "no reshape ran");
            assert_eq!(layout.offset_for_position(point(px(0.), px(42.))), at(14));
        })
        .unwrap();
    close(&mut cx);
}

#[gpui::test]
fn offset_for_position_inverts_x_on_a_scrolled_line(cx: &mut TestAppContext) {
    let long = long();
    // In the quote, an offset scrolled under the prefix maps to the prefix (D3): none is tested there.
    for (text, line, offsets) in [
        (format!("```\n{long}\n```"), 4, vec![6, 16, 64]),
        (format!("> ```\n> {long}"), 6, vec![20, 66]),
    ] {
        let (window, mut cx) = dispatching(cx, &text);
        // At the long line's start, the caret touches its `>`: the gutter is revealed and paints `> `.
        cx.simulate_keystrokes("cmd-up right right right right right right");
        cx.run_until_parked();
        let before = [line + 1, line + 2].map(|offset| x_at(window, &mut cx, offset));

        swipe_block(window, &mut cx, 0, -100.);

        window
            .update(&mut cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                let y = layout.top_for_row(1) + px(14.);
                for offset in offsets {
                    let x = layout.x_for_offset(at(offset));
                    assert_eq!(layout.offset_for_position(point(x, y)), at(offset));
                }
                if line == 6 {
                    let (prefix, text) = (layout.x_for_offset(at(7)), layout.x_for_offset(at(8)));
                    assert_eq!(prefix, before[0]);
                    assert_x(text, f32::from(before[1]) - 100.);
                    assert!(text < prefix, "{text:?} is not left of {prefix:?}");
                }
            })
            .unwrap();
        close(&mut cx);
    }
}

#[gpui::test]
fn a_caret_scrolled_out_paints_no_caret(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.simulate_keystrokes("cmd-up down end");
    cx.run_until_parked();
    // End revealed the caret; a swipe never does.
    swipe_block(window, &mut cx, 0, 1000.);
    assert_eq!(painted_caret(), None);
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.caret_bounds(view.cursor()).left(), 1152.);
        })
        .unwrap();

    swipe_block(window, &mut cx, 0, -1000.);
    assert_scrolls(window, &mut cx, 0, (412., 0.));
    assert!(painted_caret().is_some());
    close(&mut cx);
}

#[gpui::test]
fn a_caret_scrolled_out_left_paints_no_caret(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.simulate_keystrokes("cmd-up down");
    cx.run_until_parked();

    swipe_block(window, &mut cx, 0, -8.);
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let left = layout.block_backgrounds()[0].left();
            assert_eq!(layout.caret_bounds(view.cursor()).left(), left);
        })
        .unwrap();
    assert!(painted_caret().is_some(), "a caret on the edge is hidden");
    swipe_block(window, &mut cx, 0, -1.);
    assert_eq!(painted_caret(), None);
    close(&mut cx);
}

#[gpui::test]
fn an_unscrolled_block_paints_a_caret_left_of_its_background(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &format!("> ```\n> {}", long()));
    cx.simulate_keystrokes("cmd-up right right right right right right");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let left = layout.block_backgrounds()[0].left();
            let caret = layout.caret_bounds(view.cursor()).left();
            assert!(caret < left, "{caret:?} is not left of {left:?}");
        })
        .unwrap();
    assert!(painted_caret().is_some());
}

#[gpui::test]
fn a_selection_stops_at_the_background(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let quads = view
                .input
                .last_layout()
                .unwrap()
                .selection_quads(&(at(4)..at(125)));
            let right = quads
                .iter()
                .map(|quad| quad.bounds.right())
                .fold(px(f32::MIN), Pixels::max);
            assert_x(right, 764.);
        })
        .unwrap();
}

#[gpui::test]
fn a_selected_blank_line_keeps_its_stub(cx: &mut TestAppContext) {
    let long = long();
    let text = format!("```\n{long}\n\n{long}\n```\nbody");
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -100.);

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let blank = layout.top_for_row(2);
            let quads = layout.selection_quads(&(at(4)..at(200)));
            let stub = quads
                .iter()
                .find(|quad| quad.bounds.top() == blank)
                .unwrap();
            assert_x(stub.bounds.left(), -8.);
            assert_x(stub.bounds.size.width, 7.);
        })
        .unwrap();
    close(&mut cx);
}

#[gpui::test]
fn a_swipe_after_typing_scrolls_the_block(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    cx.simulate_keystrokes("a");
    cx.run_until_parked();

    swipe_block(window, &mut cx, 0, -60.);
    assert_scrolls(window, &mut cx, 0, (60., 0.));
    close(&mut cx);
}

#[gpui::test]
fn a_wheel_tick_reshapes_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let at = center(window, &mut cx, 0);

    let (shapes, lines) = (shape_count(), line_shape_count());
    swipe(&mut cx, at, point(px(-30.), px(0.)));
    assert_scrolls(window, &mut cx, 0, (30., 0.));
    assert_eq!(shape_count() - shapes, 0);
    assert_eq!(line_shape_count() - lines, 0);
    close(&mut cx);
}

#[gpui::test]
fn the_wheel_reaches_the_block_before_the_page(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    swipe_block(window, &mut cx, 0, -40.);
    assert_scrolls(window, &mut cx, 0, (40., 0.));
    close(&mut cx);
}

#[gpui::test]
fn a_scrolled_block_paints_its_text_shifted_inside_its_background(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let before = painted_clips();
    assert!(!before.is_empty(), "no block line was clipped");

    swipe_block(window, &mut cx, 0, -100.);

    let after = painted_clips();
    let width = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().block_backgrounds()[0]
                .size
                .width
        })
        .unwrap();
    assert_eq!(after.len(), before.len());
    for ((x0, mask0), (x1, mask1)) in before.iter().zip(&after) {
        assert_x(*x1, f32::from(*x0) - 100.);
        assert_eq!(mask1.right(), mask0.right());
        // Unscrolled, the text may reach left of the background; scrolled, the background cuts it.
        assert!(mask0.left() < mask1.left());
        assert_x(mask1.right() - mask1.left(), f32::from(width));
    }
    close(&mut cx);
}

#[gpui::test]
fn a_shrunk_block_reads_its_new_max_and_keeps_the_stored_x(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let start = window
        .update(&mut cx, |view, _, cx| {
            let layout = view.input.last_layout().cloned().unwrap();
            let (_, start, max, _) = layout
                .scrollable_blocks(px(f32::MIN), px(f32::MAX))
                .next()
                .unwrap();
            assert_x(max, 412.);
            let scrolls = view.scrolls.clone();
            view.buffer().update(cx, |buffer, _| {
                scrolls.set(start, px(412.), Instant::now(), buffer);
            });
            cx.notify();
            start
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            // At the line's end, so the caret's reveal asks for the clamped x and writes nothing.
            view.replace_text_in_range(Some(104..124), "", window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().cloned().unwrap();
            let (_, _, max, _) = layout
                .scrollable_blocks(px(f32::MIN), px(f32::MAX))
                .next()
                .unwrap();
            assert!(max < px(412.), "the block did not shrink: {max:?}");
            assert_eq!(view.block_x(0), max);
            assert_eq!(view.scrolls.x(start), px(412.));
        })
        .unwrap();
    close(&mut cx);
}

#[gpui::test]
fn a_swipe_on_the_block_padding_scrolls_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let at = window
        .update(&mut cx, |view, _, _| {
            let bounds = view.input.last_bounds().unwrap();
            let background = view.input.last_layout().unwrap().block_backgrounds()[0];
            bounds.origin + point(background.left() + px(3.), background.center().y)
        })
        .unwrap();
    swipe(&mut cx, at, point(px(-30.), px(0.)));
    assert_scrolls(window, &mut cx, 0, (30., 0.));
    close(&mut cx);
}

#[gpui::test]
fn an_unlocked_vertical_gesture_never_moves_the_block(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = under(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -30.), TouchPhase::Started);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-10., -2.), TouchPhase::Moved);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-10., 0.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (0., -32.));
}

#[gpui::test]
fn a_pause_of_the_gesture_gap_starts_a_new_gesture(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = under(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -30.), TouchPhase::Started);
    cx.executor().advance_clock(GESTURE_GAP);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-10., 0.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (10., -30.));
    close(&mut cx);
}

/// A gesture that starts sideways over prose, then moves by `delta` over the block.
fn sideways_then(cx: &mut TestAppContext, delta: (f32, f32), expected: (f32, f32)) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = under(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-20., 0.), TouchPhase::Started);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(delta.0, delta.1), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, expected);
    close(&mut cx);
}

#[gpui::test]
fn a_sideways_lock_holds_below_the_unlock_floor(cx: &mut TestAppContext) {
    sideways_then(cx, (-3., -5.9), (3., 0.));
}

#[gpui::test]
fn a_sideways_lock_unlocks_at_the_floor(cx: &mut TestAppContext) {
    sideways_then(cx, (-3., -6.), (0., -6.));
}

#[gpui::test]
fn a_sideways_lock_holds_below_the_unlock_ratio(cx: &mut TestAppContext) {
    sideways_then(cx, (-10., -18.9), (10., 0.));
}

#[gpui::test]
fn a_sideways_lock_unlocks_at_the_ratio(cx: &mut TestAppContext) {
    sideways_then(cx, (-10., -19.), (0., -19.));
}

#[gpui::test]
fn a_diagonal_wheel_tick_is_never_locked_sideways(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = center(window, &mut cx, 0);
    let lines = ScrollDelta::Lines(point(-2., -1.));
    wheel(&mut cx, at, lines, TouchPhase::Moved);
    let (x, y) = window
        .update(&mut cx, |view, _, _| (view.block_x(0), view.scroll_y()))
        .unwrap();
    assert_eq!(x, px(0.));
    assert!(y < px(0.), "the page scrolled: {y:?}");
}

#[gpui::test]
fn a_gesture_starting_on_the_diagonal_is_the_page_s(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-5., -5.), TouchPhase::Started);
    assert_scrolls(window, &mut cx, 0, (0., -5.));
}

#[gpui::test]
fn an_empty_start_leaves_the_next_event_to_lock(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., 0.), TouchPhase::Started);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-20., -3.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (20., 0.));
    close(&mut cx);
}

#[gpui::test]
fn an_ended_gesture_frees_the_block_for_the_page(cx: &mut TestAppContext) {
    let text = format!("{}{}", block(), tall_document());
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(-20., 0.), TouchPhase::Started);
    wheel(&mut cx, at, pixels(0., 0.), TouchPhase::Ended);
    let at = center(window, &mut cx, 0);
    wheel(&mut cx, at, pixels(0., -10.), TouchPhase::Moved);
    assert_scrolls(window, &mut cx, 0, (20., -10.));
    close(&mut cx);
}
