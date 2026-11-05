use std::sync::Arc;

use gpui::TestAppContext;
use inkra_text::Buffer;

use super::{TALL, at, dispatching, geometry, notified_after_paint, tall_document};
use crate::buffer_events::Reloaded;
use crate::editor::actions::Down;
use crate::editor::viewport::{Reveal, ScrollTo};

/// `max_offset` is non-zero only for a tracked handle on a genuinely
/// overflowing container, so it catches wiring, flex-shrink, and layout bugs alike.
#[gpui::test]
fn a_document_taller_than_the_window_has_room_to_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &"line\n".repeat(200));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.viewport.handle().max_offset().y > gpui::px(0.),
                "a 201-line document reported no room to scroll"
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_document_that_fits_has_no_room_to_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one\ntwo");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.viewport.handle().max_offset().y, gpui::px(0.));
        })
        .unwrap();
}

#[gpui::test]
fn reaching_the_end_of_a_tall_document_scrolls_it_into_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, cx| {
            let offset = view.viewport.handle().offset().y;
            assert!(offset < gpui::px(0.), "a 201-line document did not scroll");

            #[allow(clippy::cast_precision_loss)]
            let last = view.input.buffer().read(cx).line_count() as f32 - 1.;
            assert!(
                geometry::line_is_visible(last, offset),
                "the last line is at {:?}, outside the viewport",
                geometry::top_of_line(last, offset)
            );
        })
        .unwrap();
}

#[gpui::test]
fn returning_to_the_start_scrolls_back_to_the_top(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, _| {
            assert!(view.viewport.handle().offset().y < gpui::px(0.));
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.viewport.handle().offset().y, gpui::px(0.));
        })
        .unwrap();
}

/// The exact offset is asserted, not just visibility: a reveal without a
/// margin still satisfies `line_is_visible`, hiding the bug this catches.
#[gpui::test]
fn revealing_a_line_from_below_leaves_a_margin_under_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.input.buffer().read(cx).line_for_offset(view.cursor()),
                40
            );
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_showing_from_below(40.)
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_newline_at_the_bottom_edge_scrolls_the_caret_back_into_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.input.buffer().read(cx).line_for_offset(view.cursor()),
                41
            );
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_showing_from_below(41.)
            );
        })
        .unwrap();
}

/// Falsifies deriving the unscrolled top on demand as `last_bounds.origin.y -
/// viewport.handle().offset().y`: the second move's `last_bounds` predates the
/// first move's `set_offset`, so that formula would scroll twice as far.
#[gpui::test]
fn two_moves_before_a_repaint_scroll_once_each(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }

    window
        .update(&mut cx, |view, window, cx| {
            view.down(&Down, window, cx);
            view.down(&Down, window, cx);
        })
        .unwrap();

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.input.buffer().read(cx).line_for_offset(view.cursor()),
                42
            );
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_showing_from_below(42.)
            );
        })
        .unwrap();
}

#[gpui::test]
fn typing_after_a_wheel_scroll_brings_the_caret_back(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(gpui::px(17.), middle_of_line(2.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-1000.))),
        ..gpui::ScrollWheelEvent::default()
    });

    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.viewport.handle().offset().y < gpui::px(0.),
                "the wheel event did not scroll the document"
            );
        })
        .unwrap();

    cx.simulate_input("x");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.viewport.handle().offset().y, gpui::px(0.));
        })
        .unwrap();
}

#[gpui::test]
fn extending_a_selection_reveals_its_head(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("shift-down");
    }

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.input.state().selection().range().start, at(0));
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_showing_from_below(40.)
            );
        })
        .unwrap();
}

/// The shrink case gpui's own clamp cannot cover: undoing moves the caret's
/// line index without the document's end moving under it.
#[gpui::test]
fn deleting_and_restoring_lines_moves_the_caret_both_ways(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down cmd-a backspace");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.input.buffer().read(cx).line_count(), 1);
            assert_eq!(view.viewport.handle().offset().y, gpui::px(0.));
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            #[allow(clippy::cast_precision_loss)]
            let line = view.input.buffer().read(cx).line_for_offset(view.cursor()) as f32;
            let offset = view.viewport.handle().offset().y;
            assert!(offset < gpui::px(0.), "the undo did not scroll back down");
            assert!(
                geometry::line_is_visible(line, offset),
                "line {line} is at {:?} after the undo",
                geometry::top_of_line(line, offset)
            );
        })
        .unwrap();
}

/// `TextInput::hit_test` resolves against `last_bounds`, which is already
/// scroll-translated, while the click event's position is plain window space
/// — the one path where a coordinate-space mistake hides.
#[gpui::test]
fn a_click_while_scrolled_lands_on_the_line_under_the_pointer(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    let scrolled = window
        .update(&mut cx, |view, _, _| view.viewport.handle().offset().y)
        .unwrap();
    assert!(scrolled < gpui::px(0.));

    let y = gpui::px(geometry::VIEWPORT / 2.);
    let expected = geometry::line_at(y, scrolled);
    cx.simulate_click(gpui::point(gpui::px(17.), y), gpui::Modifiers::none());

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.input.buffer().read(cx).line_for_offset(view.cursor()),
                expected
            );
            assert_eq!(
                view.viewport.handle().offset().y,
                scrolled,
                "clicking a visible line scrolled the document"
            );
        })
        .unwrap();
}

/// Tall *and* wide on purpose: a wide-only document never scrolls, so the
/// assertion would hold vacuously against an implementation that writes no offset at all.
#[gpui::test]
fn revealing_never_scrolls_horizontally(cx: &mut TestAppContext) {
    let wide_and_tall = format!("{}\n", "x".repeat(2000)).repeat(TALL);
    let (window, mut cx) = dispatching(cx, &wide_and_tall);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.viewport.handle().offset().y < gpui::px(0.),
                "the document did not scroll vertically, so x was never written"
            );
            assert_eq!(view.viewport.handle().offset().x, gpui::px(0.));
            assert_eq!(
                view.viewport.handle().max_offset().x,
                gpui::px(0.),
                "a line overflowed"
            );
        })
        .unwrap();
}

/// `p_4()` padding puts the origin at (16, 16) with a 28px line height, so
/// line `n` occupies y in `[16 + 28n, 16 + 28(n+1))`. Aim at the middle of the
/// band, not its edge, so a boundary-rounding change doesn't break this.
fn middle_of_line(line: f32) -> gpui::Pixels {
    gpui::px(16. + 28. * line + 14.)
}

#[gpui::test]
fn a_click_lands_on_the_line_it_points_at(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\nccc");
    cx.run_until_parked();

    cx.simulate_click(
        gpui::point(gpui::px(17.), middle_of_line(2.)),
        gpui::Modifiers::none(),
    );
    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.input.buffer().read(cx);
            assert_eq!(buffer.line_for_offset(view.cursor()), 2);
        })
        .unwrap();
}

/// A click below a short document arrives with a y past the last row;
/// unclamped, that indexes off the end of the layout.
#[gpui::test]
fn a_click_below_the_last_line_clamps_to_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "ab\ncd");
    cx.run_until_parked();

    // Move off the target first, or the assertion would hold even if the click never arrived.
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    // Kept inside the 1920x1080 test window: a click outside it never hits the div's hitbox.
    cx.simulate_click(
        gpui::point(gpui::px(1800.), middle_of_line(5.)),
        gpui::Modifiers::none(),
    );
    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.input.buffer().read(cx);
            assert_eq!(buffer.line_for_offset(view.cursor()), 1);
            assert_eq!(view.cursor(), buffer.len());
        })
        .unwrap();
}

/// An edit reshapes in its gate, so geometry read before the next paint already describes the new text.
#[gpui::test]
fn an_edit_reshapes_before_the_next_paint(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            assert_eq!(view.input.last_layout().unwrap().row_count(), 1);
            view.newline(&crate::editor::actions::Newline, window, cx);
            assert_eq!(
                view.input.last_layout().unwrap().row_count(),
                2,
                "the layout must describe the text the edit left, not the painted one"
            );
        })
        .unwrap();
}

/// A range selected in the turn that rebinds the buffer has no geometry to scroll by yet.
#[gpui::test]
fn a_range_selected_right_after_a_rebind_is_revealed_on_the_first_paint(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "short");
    cx.run_until_parked();
    let tall = gpui::AppContext::new(&mut cx, |_| Buffer::new(tall_document()));
    let editor = window.update(&mut cx, |_, _, cx| cx.entity()).unwrap();
    let (_subscription, notified) = notified_after_paint(&editor, &mut cx);

    // `line\n` is five bytes, so line 150 starts at 750.
    window
        .update(&mut cx, |view, window, cx| {
            view.open_buffer(tall, None, cx);
            view.select_range(at(750)..at(754), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.input.state().selection().range(), at(750)..at(754));
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_centering(150.)
            );
            assert!(
                !notified.get(),
                "a notify after a paint drew a second frame"
            );
            assert_eq!(
                view.text_bounds().unwrap().top(),
                geometry::top_of_line(0., geometry::offset_centering(150.))
            );
        })
        .unwrap();
}

/// 150 lines of four bytes above line 10 of five: an unreshaped layout puts offset 650 on line
/// 130, the new text on line 160. Scrolled first, so the reload asks for its top line too.
fn scrolled_with_a_reload(
    cx: &mut TestAppContext,
) -> (
    gpui::WindowHandle<super::EditorView>,
    gpui::VisualTestContext,
    Arc<inkra_text::Diff>,
) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, cx| view.scroll_to_top(at(500), cx))
        .unwrap();
    cx.run_until_parked();
    let old = tall_document();
    let new = format!("{}{old}", "new\n".repeat(150));
    let diff = window
        .update(&mut cx, |view, _, cx| {
            view.input.buffer().update(cx, |buffer, _| {
                let diff = Arc::new(inkra_text::diff(buffer.version(), &old, &new));
                assert!(buffer.apply_diff(&diff));
                diff
            })
        })
        .unwrap();
    (window, cx, diff)
}

fn centred_on_line_160(
    window: gpui::WindowHandle<super::EditorView>,
    cx: &mut gpui::VisualTestContext,
) {
    window
        .update(cx, |view, _, _| {
            assert_eq!(view.cursor(), at(654));
            assert_eq!(
                view.viewport.handle().offset().y,
                geometry::offset_centering(160.)
            );
        })
        .unwrap();
}

/// The state between the first paint and its deferred render, when a reload lands there.
#[gpui::test]
fn a_reveal_waiting_through_a_reload_centres_by_the_new_text(cx: &mut TestAppContext) {
    let (window, mut cx, diff) = scrolled_with_a_reload(cx);

    window
        .update(&mut cx, |view, _, cx| {
            // In the text before the reload, which `on_buffer_reloaded` maps it from.
            let buffer = view.input.buffer().clone();
            let buffer = buffer.read(cx);
            view.input.state_mut().move_to(buffer, at(50), false);
            view.input.state_mut().move_to(buffer, at(54), true);
            view.viewport.request(ScrollTo::Cursor(Reveal::Center));
            let buffer = view.input.buffer().clone();
            view.on_buffer_reloaded(buffer, &Reloaded(diff), cx);
        })
        .unwrap();
    cx.run_until_parked();

    centred_on_line_160(window, &mut cx);
}

/// A hit picked after a reload, before the render that restores its top line.
#[gpui::test]
fn a_hit_picked_while_a_reload_waits_centres_by_the_new_text(cx: &mut TestAppContext) {
    let (window, mut cx, diff) = scrolled_with_a_reload(cx);

    window
        .update(&mut cx, |view, window, cx| {
            let buffer = view.input.buffer().clone();
            view.on_buffer_reloaded(buffer, &Reloaded(diff), cx);
            view.select_range(at(650)..at(654), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    centred_on_line_160(window, &mut cx);
}

/// A saved range may fall inside a cluster of the live text: `e` + U+0301, then a ZWJ family.
#[gpui::test]
fn a_selected_range_snaps_outward_to_graphemes(cx: &mut TestAppContext) {
    let text = "cafe\u{301} \u{1F468}\u{200D}\u{1F469} x";
    let (window, mut cx) = dispatching(cx, text);
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(5)..at(5), window, cx);
            assert_eq!(view.input.state().selection().range(), at(3)..at(6));
            view.select_range(at(9)..at(12), window, cx);
            assert_eq!(view.input.state().selection().range(), at(7)..at(18));
        })
        .unwrap();
}
