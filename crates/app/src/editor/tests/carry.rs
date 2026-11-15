//! A layout carries every line an edit leaves clean (incremental layout spec, D5 and D7).

use gpui::{TestAppContext, Window};
use inkra_text::Buffer;

use super::shapes::{layout_of, reshape, shape};
use super::{at, dispatching, with_editor};
use crate::editor::layout::{DocumentLayout, line_build_count};

/// The lines `f` builds rather than carries.
fn builds<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = line_build_count();
    let result = f();
    (result, line_build_count() - before)
}

/// `before`'s layout, then `after`'s carried from it in the same buffer, and the lines it built.
fn carried(before: &str, after: &str, window: &mut Window) -> (DocumentLayout, usize) {
    let mut buffer = Buffer::new(before);
    let previous = layout_of(&buffer, None, None, None, window);
    builds(|| reshape(&mut buffer, after, None, None, Some(&previous), window))
}

/// Every caret position and every line's code block match a fresh layout of `text`.
fn assert_fresh(layout: &DocumentLayout, text: &str, window: &mut Window) {
    let fresh = shape(text, None, None, None, window);
    assert_eq!(layout.height(), fresh.height(), "{text:?}");
    assert_eq!(layout.line_blocks(), fresh.line_blocks(), "{text:?}");
    assert_eq!(
        layout.block_backgrounds(),
        fresh.block_backgrounds(),
        "{text:?}"
    );
    for offset in (0..=text.len()).filter(|&offset| text.is_char_boundary(offset)) {
        assert_eq!(
            layout.position_for_offset(at(offset)),
            fresh.position_for_offset(at(offset)),
            "{text:?} at {offset}"
        );
    }
}

#[gpui::test]
fn a_keystroke_rebuilds_only_its_line(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, "# Title\n\nsome *text* here\n\nmore prose\n");
    cx.simulate_keystrokes("cmd-up down down right right");
    cx.run_until_parked();

    let ((), built) = builds(|| {
        cx.simulate_input("x");
        cx.run_until_parked();
    });
    assert_eq!(built, 1);
}

#[gpui::test]
fn a_newline_above_shifts_without_rebuilding(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "one\n\n\ntwo\n\nthree\n\nfour\n";
        let (layout, built) = carried("one\n\ntwo\n\nthree\n\nfour\n", after, window);
        assert_eq!(built, 2, "the split line and the new one");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_fence_typed_above_moves_every_block_index(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let before = "top\n\n```\na\n```\n\nmid\n\n```\nb\n```\n";
        let after = format!("```\nz\n```\n\n{before}");
        let (layout, built) = carried(before, &after, window);
        assert_eq!(
            built, 5,
            "the new block, its blank line and the line it pushed"
        );
        assert_fresh(&layout, &after, window);
    });
}

#[gpui::test]
fn opening_a_fence_rebuilds_every_line_after_it(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "a\n```\nb\n\nc\n\nd";
        let (layout, built) = carried("a\n\nb\n\nc\n\nd", after, window);
        assert_eq!(built, 6, "every line from the fence on");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_lazy_quote_line_follows_its_gutter(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "> > a\nlazy\n\nafter\n";
        let (layout, built) = carried(">  > a\nlazy\n\nafter\n", after, window);
        assert_eq!(built, 2, "the gutter line and its lazy line");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn a_changed_gutter_reaches_its_lazy_lines_and_stops(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let after = "> > a\nb\nc\n\n> d\ne\n";
        let (layout, built) = carried(">  > a\nb\nc\n\n> d\ne\n", after, window);
        assert_eq!(built, 3, "the gutter line and its two lazy lines");
        assert_fresh(&layout, after, window);
    });
}

#[gpui::test]
fn unmarking_rebuilds_the_marked_line(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let buffer = Buffer::new("zero\none two\nthree");
        let marked = layout_of(&buffer, Some(&(at(5)..at(8))), None, None, window);
        let (_, built) = builds(|| layout_of(&buffer, None, None, Some(&marked), window));
        assert_eq!(built, 1);
    });
}

#[gpui::test]
fn moving_the_marked_text_keeps_the_lines_between(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let buffer = Buffer::new("zero\none\ntwo\nthree\nfour");
        let marked = layout_of(&buffer, Some(&(at(19)..at(23))), None, None, window);
        let next = at(0)..at(4);
        let (_, built) = builds(|| layout_of(&buffer, Some(&next), None, Some(&marked), window));
        assert_eq!(built, 2, "the line left and the line marked");
    });
}

#[gpui::test]
fn a_layout_from_another_buffer_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let text = "zero\none\ntwo";
        let previous = shape(text, None, None, None, window);
        let (_, built) = builds(|| shape(text, None, None, Some(&previous), window));
        assert_eq!(built, 3);
    });
}

#[gpui::test]
fn an_overflowed_splice_log_rebuilds_the_whole_note(cx: &mut TestAppContext) {
    with_editor(cx, "", |_, window, _| {
        let mut buffer = Buffer::new("zero\none\ntwo");
        let previous = layout_of(&buffer, None, None, None, window);
        for _ in 0..257 {
            buffer.transact(|buffer| buffer.insert(at(0), "x"));
        }
        let (_, built) = builds(|| layout_of(&buffer, None, None, Some(&previous), window));
        assert_eq!(built, 3);
    });
}
