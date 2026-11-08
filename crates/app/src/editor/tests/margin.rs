//! D12's margin, measured from the heading markers, and D13's: it shrinks with the pane to a floor.

use std::rc::Rc;

use gpui::{Bounds, Modifiers, Pixels, TestAppContext, point, px, size};
use inkra_markdown::{HEADING_LEVELS, Styles};

use super::geometry::{HEADING_MARGIN, MARGIN_FLOOR, PADDING};
use super::shapes::shape;
use super::{at, dispatching, with_editor};
use crate::editor::element::{Place, Stash, column_width, margin_width};
use crate::theme::{self, builtin};

#[gpui::test]
fn every_heading_marker_fits_the_margin(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, _| {
            let style = view.input.last_layout().unwrap().style().clone();
            assert_eq!(
                margin_width(&style, builtin::dark(), window),
                px(HEADING_MARGIN)
            );
            for level in HEADING_LEVELS {
                let marker = format!("{} ", "#".repeat(level as usize));
                let run = theme::markdown_style(
                    &style,
                    Styles::heading(level) | Styles::MARKER,
                    builtin::dark(),
                )
                .to_run(marker.len());
                let width = window
                    .text_system()
                    .shape_line(
                        marker.into(),
                        theme::heading_metrics(level).size,
                        &[run],
                        None,
                    )
                    .width();
                assert!(
                    width <= px(HEADING_MARGIN),
                    "level {level} is {width:?} wide"
                );
            }
        })
        .unwrap();
}

/// The headless column and the text's bounds in a pane `column + extra` wide.
fn text_in_pane(cx: &mut TestAppContext, extra: f32) -> (f32, Bounds<Pixels>) {
    let (window, mut cx) = dispatching(cx, "abc");
    cx.run_until_parked();
    let column = window
        .update(&mut cx, |view, window, _| {
            let style = view.input.last_layout().unwrap().style().clone();
            f32::from(column_width(&style, window))
        })
        .unwrap();

    let pane = (column + extra).max(0.);
    cx.simulate_resize(size(px(2. * PADDING + pane), px(600.)));
    cx.run_until_parked();
    let bounds = window
        .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
        .unwrap();
    (column, bounds)
}

#[gpui::test]
fn a_wide_pane_centres_the_column(cx: &mut TestAppContext) {
    let (column, bounds) = text_in_pane(cx, 2. * HEADING_MARGIN + 100.);
    assert_eq!(bounds.origin.x, px(PADDING + HEADING_MARGIN + 50.));
    assert_eq!(bounds.size.width, px(column));
}

#[gpui::test]
fn a_pane_short_of_two_margins_shrinks_both(cx: &mut TestAppContext) {
    let (column, bounds) = text_in_pane(cx, 2. * HEADING_MARGIN - 40.);
    assert_eq!(bounds.origin.x, px(PADDING + HEADING_MARGIN - 20.));
    assert_eq!(bounds.size.width, px(column));
}

#[gpui::test]
fn a_narrow_pane_keeps_the_floor_on_each_side(cx: &mut TestAppContext) {
    let (column, bounds) = text_in_pane(cx, 0.);
    assert_eq!(bounds.origin.x, px(PADDING + MARGIN_FLOOR));
    assert_eq!(bounds.size.width, px(column - 2. * MARGIN_FLOOR));
}

#[gpui::test]
fn a_pane_narrower_than_both_floors_leaves_the_text_no_width(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");
    cx.simulate_resize(size(px(2. * PADDING + 2. * MARGIN_FLOOR - 10.), px(600.)));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let bounds = view.text_bounds().unwrap();
            assert_eq!(bounds.origin.x, px(PADDING + MARGIN_FLOOR));
            assert_eq!(bounds.size.width, px(0.));
            assert_eq!(view.input.last_layout().unwrap().row_count(), 3);
        })
        .unwrap();
}

#[gpui::test]
fn the_text_starts_on_a_whole_pixel_in_the_band(cx: &mut TestAppContext) {
    for extra in [41.3, 87.7, 100.9, 150.5] {
        let (_, bounds) = text_in_pane(cx, extra);
        assert_eq!(bounds.origin.x, bounds.origin.x.round(), "extra {extra}");
    }
}

/// Both entries wrap at the column, as in the band; only their margins differ.
#[gpui::test]
fn the_stash_hands_each_width_its_own_measure(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let wide = Rc::new(shape("#### T\n", None, Some(px(756.)), None, window));
        let narrow = Rc::new(shape("#### T\n", None, Some(px(756.)), None, window));
        let place = |margin: f32| Place {
            margin: px(margin),
            width: px(756.),
        };
        let stash = Stash::default();
        stash.measure(px(896.), place(70.), || Rc::clone(&wide));
        stash.measure(px(856.), place(50.), || Rc::clone(&narrow));
        stash.measure(px(856.), place(50.), || panic!("measured twice"));

        let (taken, layout) = stash.take(px(856.3)).unwrap();
        assert_eq!(taken, place(50.));
        assert!(Rc::ptr_eq(&layout, &narrow));
    });
}

/// Resolved against the element's edge instead, this click would land a margin's worth of glyphs in.
#[gpui::test]
fn a_click_at_the_text_left_edge_lands_at_the_line_start(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &"a".repeat(20));
    cx.run_until_parked();
    let column = window
        .update(&mut cx, |view, window, _| {
            let style = view.input.last_layout().unwrap().style().clone();
            column_width(&style, window)
        })
        .unwrap();

    let pane = px(1920. - 2. * PADDING);
    let left = px(PADDING) + (pane - column) / 2.;
    cx.simulate_click(point(left + px(1.), px(PADDING + 14.)), Modifiers::none());

    let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(caret, at(0));
}
