//! A view captured with `view_state()` or `capture`, restored by `open_buffer` or `restore` (tabs spec, A2 and A3).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, point, px};
use inkra_editor::{Goal, Selection};
use inkra_text::Buffer;

use super::{at, dispatching, geometry, notified_after_paint, tall_document};
use crate::editor::viewport::{Reveal, ScrollTo, Viewport};
use crate::editor::{EditorView, TopLine, ViewState};
use crate::text_input::InputHost;

fn editor(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    window.update(cx, |_, _, cx| cx.entity()).unwrap()
}

/// The view's buffer, then a note to switch away to.
fn two_notes(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
) -> (Entity<Buffer>, Entity<Buffer>) {
    let first = window
        .update(cx, |view, _, _| view.buffer().clone())
        .unwrap();
    let other = gpui::AppContext::new(cx, |_| Buffer::new("other"));
    (first, other)
}

fn open(
    window: WindowHandle<EditorView>,
    cx: &mut VisualTestContext,
    buffer: &Entity<Buffer>,
    view: Option<ViewState>,
) {
    let buffer = buffer.clone();
    window
        .update(cx, |editor, _, cx| editor.open_buffer(buffer, view, cx))
        .unwrap();
    cx.run_until_parked();
}

fn capture(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> ViewState {
    window.update(cx, |view, _, _| view.view_state()).unwrap()
}

/// The scroll offset and the painted text top.
fn drawn(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> (Pixels, Pixels) {
    window
        .update(cx, |view, _, _| {
            (view.scroll_y(), view.text_bounds().unwrap().top())
        })
        .unwrap()
}

/// A 200-line note scrolled so line 120 (`line\n` is five bytes) is on top.
fn scrolled(cx: &mut TestAppContext) -> (WindowHandle<EditorView>, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, cx| view.scroll_to_top(at(600), cx))
        .unwrap();
    cx.run_until_parked();
    (window, cx)
}

#[gpui::test]
fn a_restored_view_shows_its_top_line_on_the_first_paint(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    let (first, other) = two_notes(window, &mut cx);
    let view = capture(window, &mut cx);
    let before = drawn(window, &mut cx);
    open(window, &mut cx, &other, None);

    let (_subscription, notified) = notified_after_paint(&editor(window, &mut cx), &mut cx);
    open(window, &mut cx, &first, Some(view));

    assert_eq!(drawn(window, &mut cx), before);
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.offset_at_top(), at(600));
        })
        .unwrap();
    assert!(
        !notified.get(),
        "a notify after a paint drew a second frame"
    );
}

/// `aaa ` × 25 is 100 glyphs; the column breaks it once (`wrap.rs`), so each line is two rows.
#[gpui::test]
fn a_wrapped_top_row_keeps_its_offset_within_the_row(cx: &mut TestAppContext) {
    let text = format!("{}\n", "aaa ".repeat(25)).repeat(100);
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    // Eleven whole pixels into row 61: a fraction of a pixel would come back snapped.
    window
        .update(&mut cx, |view, _, cx| {
            let layout = view.input.last_layout().unwrap().clone();
            let row_start = *layout.row_bounds(61).start();
            view.scroll_to_top(row_start, cx);
            let y = view.scroll_y() - px(11.);
            view.viewport.handle().set_offset(point(px(0.), y));
        })
        .unwrap();
    cx.run_until_parked();
    let (first, other) = two_notes(window, &mut cx);
    let view = capture(window, &mut cx);
    let before = drawn(window, &mut cx);

    assert!(view.top.below() > px(0.), "the capture lost the part-row");
    let offset = view.top.offset().get();
    assert!(
        offset > 0 && !text[..offset].ends_with('\n'),
        "the top row should be a wrapped one, not a line start"
    );

    open(window, &mut cx, &other, None);
    open(window, &mut cx, &first, Some(view));

    assert_eq!(drawn(window, &mut cx), before);
}

#[gpui::test]
fn a_reversed_selection_keeps_its_direction_and_goal(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &"a line of text\n".repeat(10));
    cx.run_until_parked();
    let (first, other) = two_notes(window, &mut cx);
    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(81)..at(81), window, cx);
        })
        .unwrap();
    cx.simulate_keystrokes("shift-up shift-up");
    let view = capture(window, &mut cx);
    assert!(view.selection.reversed());
    assert!(matches!(view.selection.goal(), Goal::Column(_)));

    open(window, &mut cx, &other, None);
    open(window, &mut cx, &first, Some(view.clone()));

    assert_eq!(capture(window, &mut cx).selection, view.selection);
}

#[gpui::test]
fn a_rebind_drops_a_composition_and_keeps_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "ab");
    cx.run_until_parked();
    let (first, other) = two_notes(window, &mut cx);
    window
        .update(&mut cx, |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        })
        .unwrap();
    let view = capture(window, &mut cx);

    open(window, &mut cx, &other, None);
    window
        .update(&mut cx, |editor, _, cx| {
            assert_eq!(editor.input.state().marked_range(), None);
            assert_eq!(first.read(cx).text(), "abか");
        })
        .unwrap();

    open(window, &mut cx, &first, Some(view.clone()));
    window
        .update(&mut cx, |editor, _, _| {
            assert_eq!(editor.input.state().marked_range(), None);
            assert_eq!(editor.input.state().selection(), &view.selection);
        })
        .unwrap();
}

#[gpui::test]
fn two_captures_in_one_frame_read_the_waiting_request(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    let (first, other) = two_notes(window, &mut cx);
    let view = capture(window, &mut cx);
    open(window, &mut cx, &other, None);

    window
        .update(&mut cx, |editor, _, cx| {
            editor.open_buffer(first, Some(view.clone()), cx);
            assert_eq!(editor.view_state(), view);
        })
        .unwrap();
}

/// `e` + U+0301 is one grapheme at 3..6; the note ends at 8.
#[gpui::test]
fn a_restored_view_clips_to_the_note(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "short");
    cx.run_until_parked();
    let note = gpui::AppContext::new(&mut cx, |_| Buffer::new("cafe\u{301} x"));
    let mut selection = Selection::at(at(40));
    selection.set_head(at(5), Goal::Column(3.));
    let view = ViewState {
        selection,
        top: TopLine::at(at(40)),
        scrolls: Vec::new(),
    };

    window
        .update(&mut cx, |editor, _, cx| {
            editor.open_buffer(note, Some(view), cx);
            let restored = editor.view_state();
            assert_eq!(restored.selection.range(), at(3)..at(8));
            assert!(restored.selection.reversed());
            assert_eq!(restored.selection.goal(), Goal::Column(3.));
            assert_eq!(restored.top.offset(), at(8));
        })
        .unwrap();
}

/// As `scroll.rs`'s `scrolled_with_a_reload`: the diff lands with no notify, after the rebind.
#[gpui::test]
fn a_restore_onto_a_buffer_reloaded_without_a_notify_reads_the_new_text(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    let (first, other) = two_notes(window, &mut cx);
    let view = capture(window, &mut cx);
    open(window, &mut cx, &other, None);
    let old = tall_document();
    // Headings above the top line: stale spans would lay them out as body rows.
    let new = format!("{}{old}", "# h\n".repeat(30));

    window
        .update(&mut cx, |editor, _, cx| {
            editor.open_buffer(first.clone(), Some(view), cx);
            first.update(cx, |buffer, _| {
                let diff = Arc::new(inkra_text::diff(buffer.version(), &old, &new));
                assert!(buffer.apply_diff(&diff));
            });
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |editor, _, _| {
            assert_eq!(editor.spans(), inkra_markdown::parse(&new).spans.as_slice());
            assert_eq!(editor.offset_at_top(), at(600));
        })
        .unwrap();
}

/// Swaps in a never-prepainted viewport: no window gives the scroll div zero height (its padding stays).
#[gpui::test]
fn a_reveal_with_no_viewport_height_waits_for_a_frame_with_one(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(750)..at(750), window, cx);
            let layout = view.input.last_layout().unwrap().clone();
            let mut fresh = Viewport::default();
            fresh.request(ScrollTo::Cursor(Reveal::Center));
            let origin = view.text_bounds().unwrap().top();
            assert_eq!(fresh.resolve(origin, &layout, at(750)).moved, px(0.));
            view.viewport = fresh;
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        window
            .update(&mut cx, |view, _, _| view.scroll_y())
            .unwrap(),
        geometry::offset_centering(150.)
    );
}

/// Paint must store the bounds prepaint shifted: the frame's own parameter is a scroll behind.
#[gpui::test]
fn a_click_after_a_restore_hits_the_restored_text(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    let (first, other) = two_notes(window, &mut cx);
    let view = capture(window, &mut cx);
    open(window, &mut cx, &other, None);
    open(window, &mut cx, &first, Some(view));

    let (scroll_y, _) = drawn(window, &mut cx);
    let left = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap().left())
        .unwrap();
    let row_middle = geometry::top_of_line(120., scroll_y) + px(geometry::LINE_HEIGHT / 2.);
    cx.simulate_click(point(left + px(1.), row_middle), Modifiers::none());

    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(600)))
        .unwrap();
}

/// A 44 px heading row restored as a 28 px body row: the part-row captured on it must stop at its end.
#[gpui::test]
fn a_restored_top_line_stops_at_the_end_of_a_row_that_got_shorter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(
        cx,
        &format!("{}# Title\n{}", "line\n".repeat(50), tall_document()),
    );
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, cx| {
            let layout = view.input.last_layout().unwrap().clone();
            let heading = layout.row_height(layout.row_for_offset(at(250)));
            view.scroll_to_top(at(250), cx);
            let y = view.scroll_y() - (heading - px(4.));
            view.viewport.handle().set_offset(point(px(0.), y));
        })
        .unwrap();
    cx.run_until_parked();
    let view = capture(window, &mut cx);
    assert!(
        view.top.below() > px(geometry::LINE_HEIGHT),
        "the capture must sit past a body row's height"
    );
    let body = gpui::AppContext::new(&mut cx, |_| {
        Buffer::new(format!(
            "{}x Title\n{}",
            "line\n".repeat(50),
            tall_document()
        ))
    });

    open(window, &mut cx, &body, Some(view));

    let (scroll_y, _) = drawn(window, &mut cx);
    assert_eq!(
        geometry::top_of_line(51., scroll_y),
        px(0.),
        "the next line must start at the top, not be cut"
    );
}

fn capture_sealing(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> ViewState {
    window.update(cx, |view, _, cx| view.capture(cx)).unwrap()
}

fn text(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

#[gpui::test]
fn a_capture_seals_the_undo_group(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("x");
    capture_sealing(window, &mut cx);
    cx.simulate_input("y");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(window, &mut cx), "x");
}

/// A buffer notify is what schedules an autosave.
#[gpui::test]
fn a_capture_of_a_clean_note_leaves_it_clean(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "note");
    cx.run_until_parked();
    let buffer = window
        .update(&mut cx, |view, _, _| view.buffer().clone())
        .unwrap();
    let notified = Rc::new(Cell::new(false));
    let set = Rc::clone(&notified);
    let _subscription = cx.update(|_, cx| cx.observe(&buffer, move |_, _| set.set(true)));

    capture_sealing(window, &mut cx);
    cx.run_until_parked();

    assert!(!notified.get(), "the capture notified the buffer");
}

#[gpui::test]
fn restore_on_the_same_buffer_keeps_its_parse(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(610)..at(612), window, cx);
        })
        .unwrap();

    let view = capture_sealing(window, &mut cx);
    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(5)..at(5), window, cx);
            view.scroll_to_top(at(0), cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |editor, _, cx| {
            let parsed_at = editor.parsed_at;
            // No notify: a reparse inside `restore` is the only thing that would read it.
            editor.buffer().update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.insert(at(1000), "x"));
            });
            editor.restore(view.clone(), cx);
            assert_eq!(editor.parsed_at, parsed_at, "restore reparsed");
            assert_eq!(editor.view_state(), view);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(&mut cx, |editor, _, _| {
            assert_eq!(editor.selected(), at(610)..at(612));
            assert_eq!(editor.offset_at_top(), view.top.offset());
        })
        .unwrap();
}

#[gpui::test]
fn restore_drops_a_composition_and_keeps_its_text(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "ab");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        })
        .unwrap();
    let view = capture_sealing(window, &mut cx);

    window
        .update(&mut cx, |editor, _, cx| {
            editor.restore(view.clone(), cx);
            assert_eq!(editor.input.state().marked_range(), None);
            assert_eq!(editor.input.state().selection(), &view.selection);
        })
        .unwrap();
    assert_eq!(text(window, &mut cx), "abか");
}

/// `e` + U+0301 is one grapheme at 3..6; the note ends at 8.
#[gpui::test]
fn a_view_restored_onto_a_shorter_note_is_clipped(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    let mut selection = Selection::at(at(700));
    selection.set_head(at(5), Goal::Column(3.));
    let view = ViewState {
        selection,
        top: TopLine::at(at(600)),
        scrolls: Vec::new(),
    };

    window
        .update(&mut cx, |editor, _, cx| {
            // Shortened as another tab on this note would: before the view comes back.
            editor.buffer().update(cx, |buffer, cx| {
                buffer.transact(|buffer| buffer.replace(at(0)..buffer.len(), "cafe\u{301} x"));
                cx.notify();
            });
            editor.restore(view, cx);
            let restored = editor.view_state();
            assert_eq!(restored.selection.range(), at(3)..at(8));
            assert!(restored.selection.reversed());
            assert_eq!(restored.top.offset(), at(8));
        })
        .unwrap();
    cx.run_until_parked();
}

fn park_and_unpark(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> ViewState {
    window
        .update(cx, |view, _, cx| {
            let parked = view.park(cx);
            parked.unpark(cx)
        })
        .unwrap()
}

#[gpui::test]
fn park_then_unpark_with_no_edit_is_the_view_captured(cx: &mut TestAppContext) {
    let (window, mut cx) = scrolled(cx);
    window
        .update(&mut cx, |view, window, cx| {
            view.select_range(at(612)..at(612), window, cx);
            view.scroll_to_top(at(600), cx);
        })
        .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-up shift-up");
    let view = capture(window, &mut cx);
    assert!(view.selection.reversed());
    assert!(matches!(view.selection.goal(), Goal::Column(_)));
    assert_ne!(view.top, TopLine::default());

    assert_eq!(park_and_unpark(window, &mut cx), view);
}

#[gpui::test]
fn park_seals_the_undo_group(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("x");
    park_and_unpark(window, &mut cx);
    cx.simulate_input("y");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(window, &mut cx), "x");
}

/// The delete notifies no one, so the restored top line still waits, past the new end.
#[gpui::test]
fn park_clips_a_waiting_top_line_past_the_end(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    let unparked = window
        .update(&mut cx, |editor, _, cx| {
            editor.restore(
                ViewState {
                    selection: Selection::at(at(0)),
                    top: TopLine::at(at(600)),
                    scrolls: Vec::new(),
                },
                cx,
            );
            editor.buffer().update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.delete(at(10)..buffer.len()));
            });
            let parked = editor.park(cx);
            parked.unpark(cx)
        })
        .unwrap();

    assert_eq!(unparked.top.offset(), at(10));
}
