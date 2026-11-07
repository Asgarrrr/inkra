//! A thin bar shows a scrollable block's position while it is hovered and for a moment after it
//! scrolls (code block scroll spec, S3). Headless, a block's `max` is 412 against a 756 px column.

use gpui::{Modifiers, Pixels, Point, TestAppContext, VisualTestContext, WindowHandle, point, px};
use inkra_editor::Selection;

use super::code_block_scroll::{block, swipe_block};
use super::{assert_x, at, close, dispatching};
use crate::editor::block_scroll::BAR_LINGER;
use crate::editor::element::painted_bars;
use crate::editor::{EditorView, TopLine, ViewState};

fn move_to(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
}

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

/// The prose row under block 0, read again before each move.
fn off(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> Point<Pixels> {
    window
        .update(cx, |view, _, _| {
            let bounds = view.text_bounds().unwrap();
            let background = view.input.last_layout().unwrap().block_backgrounds()[0];
            bounds.origin + point(background.center().x, background.bottom() + px(14.))
        })
        .unwrap()
}

/// Block 0's background bottom and the text's left, in window coordinates.
fn frame(window: WindowHandle<EditorView>, cx: &mut VisualTestContext) -> (Pixels, Pixels) {
    window
        .update(cx, |view, _, _| {
            let bounds = view.text_bounds().unwrap();
            let background = view.input.last_layout().unwrap().block_backgrounds()[0];
            (bounds.top() + background.bottom(), bounds.left())
        })
        .unwrap()
}

#[gpui::test]
fn the_bar_shows_after_a_swipe(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -100.);
    let away = off(window, &mut cx);
    move_to(&mut cx, away);

    let bars = painted_bars();
    let (bottom, left) = frame(window, &mut cx);
    assert_eq!(bars.len(), 1);
    assert_x(bars[0].size.width, 489.33);
    assert_x(bars[0].left() - left, 64.73);
    assert_x(bars[0].size.height, 3.);
    assert_x(bars[0].bottom(), f32::from(bottom) - 1.);
    close(&mut cx);
}

#[gpui::test]
fn the_bar_hides_after_the_linger(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    swipe_block(window, &mut cx, 0, -100.);
    let away = off(window, &mut cx);
    move_to(&mut cx, away);
    assert_eq!(painted_bars().len(), 1);

    cx.executor().advance_clock(BAR_LINGER);
    cx.run_until_parked();

    assert!(painted_bars().is_empty());
    close(&mut cx);
}

#[gpui::test]
fn hovering_a_wide_block_shows_its_bar(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    assert!(painted_bars().is_empty());

    let at = center(window, &mut cx, 0);
    move_to(&mut cx, at);
    let bars = painted_bars();
    let (bottom, left) = frame(window, &mut cx);
    assert_eq!(bars.len(), 1);
    assert_x(bars[0].left() - left, 0.);
    assert_x(bars[0].bottom(), f32::from(bottom) - 1.);

    let away = off(window, &mut cx);
    move_to(&mut cx, away);
    assert!(painted_bars().is_empty());
}

#[gpui::test]
fn a_block_that_fits_has_no_bar(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "```\nshort\n```\nbody");
    cx.run_until_parked();

    let at = center(window, &mut cx, 0);
    move_to(&mut cx, at);

    assert!(painted_bars().is_empty());
}

#[gpui::test]
fn a_restored_scroll_shows_the_bar_until_the_linger(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let view = ViewState {
        selection: Selection::at(at(0)),
        top: TopLine::default(),
        scrolls: vec![(at(0), px(60.))],
    };
    window
        .update(&mut cx, |editor, _, cx| editor.restore(view, cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(painted_bars().len(), 1);

    cx.executor().advance_clock(BAR_LINGER);
    cx.run_until_parked();

    assert!(painted_bars().is_empty());
    close(&mut cx);
}

#[gpui::test]
fn a_move_that_changes_no_hover_never_repaints(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &block());
    cx.run_until_parked();
    let away = off(window, &mut cx);
    move_to(&mut cx, away);
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    let view = window.root(&mut cx).unwrap();
    let _observer = cx.update(|_, cx| cx.observe(&view, move |_, _| count.set(count.get() + 1)));
    move_to(&mut cx, away + point(px(5.), px(0.)));
    assert_eq!(notified.get(), 0);
    let over = center(window, &mut cx, 0);
    move_to(&mut cx, over);
    assert_eq!(notified.get(), 1);
}
