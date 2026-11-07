//! Code block lines, backgrounds and labels (code blocks spec, C1-C3). Headless, a glyph is 0.6 em wide.

use gpui::{Bounds, Pixels, TestAppContext, px};

use super::shapes::shape;
use super::{TALL, assert_x, at, dispatching, with_editor};
use crate::editor::layout::{label_shape_count, painted_blocks, shape_count};

const CODE_GLYPH: f32 = 0.6 * 16.;
const BODY_GLYPH: f32 = 0.6 * 18.;

#[gpui::test]
fn a_block_line_shapes_at_the_code_size_in_a_body_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "```rs\nlet x;\n```\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.x_for_offset(at(10)), 4. * CODE_GLYPH);
            assert_eq!(layout.row_height(1), px(28.));
            assert_x(layout.x_for_offset(at(19)), 2. * BODY_GLYPH);
        })
        .unwrap();
}

#[gpui::test]
fn the_line_between_two_blocks_keeps_the_body_size(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "```\na\n```\nmid\n```\nb\n```");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_x(layout.x_for_offset(at(5)), CODE_GLYPH);
            assert_x(layout.x_for_offset(at(11)), BODY_GLYPH);
            assert_x(layout.x_for_offset(at(19)), CODE_GLYPH);
        })
        .unwrap();
}

#[gpui::test]
fn a_caret_in_the_block_reveals_both_fences_and_moves_no_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "```rs\nlet x;\n```\nbody");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let concealed = window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(0), "");
            assert_eq!(layout.shown(2), "");
            let tops: Vec<_> = (0..4).map(|row| layout.top_for_row(row)).collect();
            (layout.height(), tops)
        })
        .unwrap();
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(0), "```rs");
            assert_eq!(layout.shown(2), "```");
            let tops: Vec<_> = (0..4).map(|row| layout.top_for_row(row)).collect();
            assert_eq!((layout.height(), tops), concealed);
        })
        .unwrap();
}

#[gpui::test]
fn entering_a_block_reshapes_once(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "```rs\nlet x;\nlet y;\n```\nbody");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

#[gpui::test]
fn moving_inside_a_revealed_block_shapes_nothing(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "```rs\nlet x;\nlet y;\n```\nbody");
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 0);
}

#[gpui::test]
fn a_fence_in_a_quote_hides_after_its_gutter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "> ```sh\n> x\n> ```\nbody");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.shown(0), "");
            assert_eq!(layout.shown(2), "");
            assert_eq!(layout.quote_bars().len(), 3);
        })
        .unwrap();
}

/// The backgrounds of `text` in the real editor, caret at its end.
fn backgrounds(cx: &mut TestAppContext, text: &str) -> Vec<Bounds<Pixels>> {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().block_backgrounds()
        })
        .unwrap()
}

/// One background with these edges; every right edge is the column's 756 px plus the outset.
fn assert_block(backgrounds: &[Bounds<Pixels>], left: f32, top: Option<f32>, bottom: Option<f32>) {
    assert_eq!(backgrounds.len(), 1);
    let block = backgrounds[0];
    assert_x(block.left(), left);
    assert_x(block.right(), 764.);
    if let Some(top) = top {
        assert_x(block.top(), top);
    }
    if let Some(bottom) = bottom {
        assert_x(block.bottom(), bottom);
    }
}

#[gpui::test]
fn a_background_spans_its_block_from_the_first_top_to_the_last_bottom(cx: &mut TestAppContext) {
    let text = format!("```\n{}\n```\nbody", "aaaa ".repeat(20));
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_block(&layout.block_backgrounds(), -8., Some(0.), Some(84.));
            assert_eq!(layout.row_count(), 4);
        })
        .unwrap();
}

#[gpui::test]
fn a_quoted_background_starts_past_the_gutter(cx: &mut TestAppContext) {
    let blocks = backgrounds(cx, "> ```\n> x\n> ```\nbody");
    assert_block(&blocks, 13.6, Some(0.), Some(84.));
}

#[gpui::test]
fn a_listed_background_starts_at_the_fence_column(cx: &mut TestAppContext) {
    let blocks = backgrounds(cx, "- a\n\n  ```\n  x\n  ```\nbody");
    assert_block(&blocks, 11.2, Some(56.), None);
}

#[gpui::test]
fn an_indented_background_starts_at_the_indent(cx: &mut TestAppContext) {
    let blocks = backgrounds(cx, "    code\n    more\n\nbody");
    assert_block(&blocks, 30.4, None, Some(56.));
}

#[gpui::test]
fn a_listed_block_in_a_quote_starts_past_both(cx: &mut TestAppContext) {
    let blocks = backgrounds(cx, "> - a\n>\n>   ```\n>   x\n>   ```\n\nbody");
    assert_block(&blocks, 32.8, None, None);
}

#[gpui::test]
fn two_blocks_get_two_backgrounds_in_order(cx: &mut TestAppContext) {
    let blocks = backgrounds(cx, "```\na\n```\nmid\n```\nb\n```");
    let edges: Vec<_> = blocks
        .iter()
        .map(|block| (f32::from(block.top()), f32::from(block.bottom())))
        .collect();
    assert_eq!(edges, [(0., 84.), (112., 196.)]);
}

#[gpui::test]
fn an_unclosed_block_gets_its_background(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "body\n```rs\nx");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let blocks = layout.block_backgrounds();
            assert_eq!(blocks.len(), 1);
            assert_x(blocks[0].top(), 28.);
            assert_x(blocks[0].bottom(), f32::from(layout.height()));
        })
        .unwrap();
}

#[gpui::test]
fn without_a_column_the_background_ends_past_its_widest_line(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let layout = shape("```\nabcdef\n```", None, None, None, window);
        let blocks = layout.block_backgrounds();
        assert_eq!(blocks.len(), 1);
        assert_x(blocks[0].right(), 6. * CODE_GLYPH + 8.);
    });
}

#[gpui::test]
fn only_visible_blocks_paint(cx: &mut TestAppContext) {
    let text = format!(
        "```\na\n```\n{}```\n{}```",
        "line\n".repeat(TALL),
        "x\n".repeat(TALL)
    );
    let (_window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    assert_eq!(painted_blocks(), 0..1);

    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    assert_eq!(painted_blocks(), 1..2);
}

/// The labels of `text` in the real editor after `keys`, as `(x, y, text)`.
fn labels(cx: &mut TestAppContext, text: &str, keys: &str) -> Vec<Option<(f32, f32, String)>> {
    let (window, mut cx) = dispatching(cx, text);
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            view.input
                .last_layout()
                .unwrap()
                .block_labels()
                .into_iter()
                .map(|label| label.map(|(at, text)| (at.x.into(), at.y.into(), text)))
                .collect()
        })
        .unwrap()
}

/// One label, `0.6 * 12` px per glyph: the background's right edge is 764, less the outset.
fn assert_labels(found: &[Option<(f32, f32, String)>], expected: &[Option<(f32, f32, &str)>]) {
    assert_eq!(found.len(), expected.len());
    for (found, expected) in found.iter().zip(expected) {
        match (found, expected) {
            (Some((x, y, text)), Some((left, top, name))) => {
                assert_x(px(*x), *left);
                assert_x(px(*y), *top);
                assert_eq!(text, name);
            }
            (None, None) => {}
            _ => panic!("labels differ: {found:?} against {expected:?}"),
        }
    }
}

#[gpui::test]
fn a_block_with_a_language_labels_its_opening_row(cx: &mut TestAppContext) {
    let found = labels(cx, "```rust ignore\nlet x;\n```\nbody", "cmd-down");
    assert_labels(&found, &[Some((727.2, 0., "rust"))]);
}

#[gpui::test]
fn a_revealed_block_has_no_label(cx: &mut TestAppContext) {
    let found = labels(cx, "```rust ignore\nlet x;\n```\nbody", "cmd-up");
    assert_labels(&found, &[None]);
}

#[gpui::test]
fn a_quoted_block_labels_at_the_same_right_edge(cx: &mut TestAppContext) {
    let found = labels(cx, "> ```sh\n> x\n> ```\nbody", "cmd-down");
    assert_labels(&found, &[Some((741.6, 0., "sh"))]);
}

#[gpui::test]
fn no_language_and_an_indented_block_have_no_label(cx: &mut TestAppContext) {
    let found = labels(cx, "```\nx\n```\n\n    code\n\nbody", "cmd-down");
    assert_labels(&found, &[None, None]);
}

#[gpui::test]
fn each_label_text_shapes_once_per_layout(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let before = label_shape_count();
        let text = "body\n```rust\na\n```\n```rust\nb\n```\n```py\nc\n```";
        let layout = shape(text, None, Some(px(756.)), None, window);
        assert_eq!(label_shape_count() - before, 2);
        assert!(layout.block_labels().iter().all(Option::is_some));
    });
}
