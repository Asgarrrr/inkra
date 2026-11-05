//! D12's margin: reserved left of the text column, measured from the heading markers.

use gpui::{Bounds, Modifiers, Pixels, TestAppContext, point, px, size};
use inkra_markdown::{HEADING_LEVELS, Styles};

use super::geometry::{HEADING_MARGIN, PADDING};
use super::{at, dispatching};
use crate::editor::element::{column_width, margin_width};
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

/// Centred, this column would start 20 px short of the margin.
#[gpui::test]
fn a_pane_short_of_two_margins_shifts_the_full_column_right(cx: &mut TestAppContext) {
    let (column, bounds) = text_in_pane(cx, 2. * HEADING_MARGIN - 40.);
    assert_eq!(bounds.origin.x, px(PADDING + HEADING_MARGIN));
    assert_eq!(bounds.size.width, px(column));
}

#[gpui::test]
fn a_narrow_pane_loses_the_margin_on_the_left_only(cx: &mut TestAppContext) {
    let (column, bounds) = text_in_pane(cx, 0.);
    assert_eq!(bounds.origin.x, px(PADDING + HEADING_MARGIN));
    assert_eq!(bounds.size.width, px(column - HEADING_MARGIN));
}

#[gpui::test]
fn a_pane_narrower_than_the_margin_leaves_the_text_no_width(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");
    cx.simulate_resize(size(px(2. * PADDING + HEADING_MARGIN - 20.), px(600.)));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let bounds = view.text_bounds().unwrap();
            assert_eq!(bounds.origin.x, px(PADDING + HEADING_MARGIN));
            assert_eq!(bounds.size.width, px(0.));
            assert_eq!(view.input.last_layout().unwrap().row_count(), 3);
        })
        .unwrap();
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
