//! A frame reuses the layout its inputs already produced (roadmap, "One shape per keystroke").

use std::ops::Range;
use std::sync::Arc;

use gpui::{Pixels, TestAppContext, TextStyle, Window, WindowAppearance, prelude::*, px};
use inkra_text::{Buffer, ByteOffset};

use super::{at, dispatching, window_on, with_editor};
use crate::editor::layout::{
    DocumentLayout, LayoutInputs, line_shape_count, shape_count, shape_document,
};
use crate::theme::{self, Appearance, Theme, ThemeColors, builtin};

#[gpui::test]
fn a_keystroke_shapes_the_note_once(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "# Title\n\nsome *text* and [[a link]]\n");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_input("x");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

#[gpui::test]
fn a_keystroke_hands_only_its_line_to_shape_text(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "zero\none\ntwo\nthree\n");
    cx.run_until_parked();

    let before = line_shape_count();
    cx.simulate_input("x");
    cx.run_until_parked();
    assert_eq!(line_shape_count() - before, 1);
}

/// Ten distinct lines, so a same-index match cannot pass by coincidence.
const TEN_LINES: &str =
    "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\n";

#[gpui::test]
fn a_newline_hands_only_its_own_line_to_shape_text(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, TEN_LINES);
    cx.simulate_keystrokes("cmd-up down down");
    cx.run_until_parked();

    let before = line_shape_count();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(line_shape_count() - before, 1);
}

#[gpui::test]
fn joining_two_lines_hands_only_the_joined_line_to_shape_text(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, TEN_LINES);
    cx.simulate_keystrokes("cmd-up down down");
    cx.run_until_parked();

    let before = line_shape_count();
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(line_shape_count() - before, 1);
}

/// `text` shaped from scratch, or carrying what it can from `previous`.
fn shape(
    text: &str,
    marked: Option<&Range<ByteOffset>>,
    wrap_width: Option<Pixels>,
    previous: Option<&DocumentLayout>,
    window: &mut Window,
) -> DocumentLayout {
    let buffer = Buffer::new(text);
    let markdown = inkra_markdown::parse(text);
    let style = window.text_style();
    let inputs = LayoutInputs {
        buffer: &buffer,
        markdown: &markdown,
        selection: &(at(0)..at(0)),
        marked,
        style: &style,
        wrap_width,
        theme: builtin::dark(),
    };
    shape_document(&inputs, previous, window)
}

#[gpui::test]
fn a_carried_line_matches_a_fresh_shape(cx: &mut TestAppContext) {
    let long = "word ".repeat(24);
    let cases = [
        ("Title\n===\nbody\n", "Title\n---\nbody\n", None, None),
        (long.as_str(), long.as_str(), Some(px(300.)), Some(px(200.))),
    ];
    with_editor(cx, "", |_, window, _| {
        for (before, after, wrap_before, wrap_after) in cases {
            let previous = shape(before, None, wrap_before, None, window);
            let carried = shape(after, None, wrap_after, Some(&previous), window);
            let fresh = shape(after, None, wrap_after, None, window);

            assert_eq!(carried.row_count(), fresh.row_count(), "{after:?}");
            assert_eq!(carried.height(), fresh.height(), "{after:?}");
            for offset in (0..=after.len()).filter(|&offset| after.is_char_boundary(offset)) {
                assert_eq!(
                    carried.position_for_offset(at(offset)),
                    fresh.position_for_offset(at(offset)),
                    "{after:?} at {offset}"
                );
            }
        }
    });
}

#[gpui::test]
fn a_composition_reshapes_only_its_line(cx: &mut TestAppContext) {
    let text = "zero\none two\nthree";
    with_editor(cx, "", |_, window, _| {
        let previous = shape(text, None, None, None, window);
        let before = line_shape_count();
        shape(text, Some(&(at(5)..at(8))), None, Some(&previous), window);
        assert_eq!(line_shape_count() - before, 1);
    });
}

#[gpui::test]
fn a_caret_move_that_reveals_nothing_shapes_nothing(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "plain words\n");
    cx.run_until_parked();

    let before = shape_count();
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 0);
}

#[gpui::test]
fn a_theme_change_reshapes_the_note_once(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "[a](b)\n");
    cx.run_until_parked();

    let before = shape_count();
    let red = Arc::new(Theme {
        appearance: Appearance::Dark,
        colors: ThemeColors {
            md_link: gpui::red().into(),
            ..builtin::dark().colors
        },
    });
    cx.update(|_, cx| theme::install(Some(red.clone()), cx));
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
    window
        .update(&mut cx, |view, _, _| {
            assert!(Arc::ptr_eq(view.input.last_layout().unwrap().theme(), &red));
        })
        .unwrap();
}

#[gpui::test]
fn the_system_appearance_reshapes_with_its_theme(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "[a](b)\n");
    cx.run_until_parked();

    for (system, appearance) in [
        (WindowAppearance::VibrantLight, Appearance::Light),
        (WindowAppearance::Dark, Appearance::Dark),
    ] {
        cx.update(|_, cx| theme::follow_system(system, cx));
        cx.run_until_parked();
        window
            .update(&mut cx, |view, _, _| {
                let shown = view.input.last_layout().unwrap().theme();
                assert!(Arc::ptr_eq(shown, builtin::of(appearance)), "{system:?}");
            })
            .unwrap();
    }
}

#[gpui::test]
fn an_equal_theme_changes_nothing(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "[a](b)\n");
    cx.run_until_parked();

    let before = shape_count();
    cx.update(|_, cx| {
        theme::install(Some(Arc::new((**builtin::dark()).clone())), cx);
        assert!(Arc::ptr_eq(theme::current(cx), builtin::dark()));
    });
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 0);
}

#[gpui::test]
fn a_write_outside_the_gates_reaches_the_next_paint(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("b")));
    let window = window_on(cx, buffer.clone());
    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(ByteOffset::new(1), "c");
        });
        cx.notify();
    });
    cx.run_until_parked();

    window
        .update(cx, |view, _, _| {
            assert_eq!(view.input.last_layout().unwrap().shown(0), "bc");
        })
        .unwrap();
}

/// No gated path changes the style or reveals an element without reshaping, so only this test reaches them.
#[gpui::test]
fn a_layout_is_reused_only_for_inputs_that_shape_it_alike(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one *two* three\n");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.input.buffer().read(cx);
            let layout = view.input.last_layout().unwrap();
            let (style, width) = (layout.style().clone(), layout.wrap_width());
            let reused = |selection: Range<ByteOffset>, marked: Option<Range<ByteOffset>>| {
                layout.is_shaped_from(&LayoutInputs {
                    buffer,
                    markdown: view.markdown(),
                    selection: &selection,
                    marked: marked.as_ref(),
                    style: &style,
                    wrap_width: width,
                    theme: builtin::dark(),
                })
            };
            // `last_layout` still holds the end-of-document caret the window opened with.
            assert!(reused(at(0)..at(0), None), "revealing nothing, as before");
            assert!(
                !reused(at(6)..at(6), None),
                "inside `*two*`, which reveals it"
            );
            assert!(!reused(at(0)..at(0), Some(at(0)..at(3))), "a composition");

            let mut bigger = style.clone();
            bigger.font_size = px(40.).into();
            let other = |style: &TextStyle| {
                layout.is_shaped_from(&LayoutInputs {
                    buffer,
                    markdown: view.markdown(),
                    selection: &(at(0)..at(0)),
                    marked: None,
                    style,
                    wrap_width: width,
                    theme: builtin::dark(),
                })
            };
            assert!(!other(&bigger), "another style");
        })
        .unwrap();
}
