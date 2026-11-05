use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext, Context, Entity, Focusable, InputHandler, IntoElement, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use inkra_text::{Buffer, ByteOffset};

use super::{InputAdapter, InputHost};
use crate::editor::EditorView;
use crate::field::NameField;
use crate::keymap::key_bindings;

fn field(cx: &mut TestAppContext, initial: &str) -> (Entity<NameField>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let initial = initial.to_owned();
    let window = cx.add_window(move |_, cx| NameField::new(&initial, cx));
    window
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle(cx), cx);
        })
        .unwrap();
    let view = window.root(cx).expect("root view");
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    (view, visual)
}

fn editor(cx: &mut TestAppContext, initial: &str) -> (Entity<EditorView>, VisualTestContext) {
    let buffer = cx.new(|_| Buffer::new(initial));
    let window = cx.add_window(move |_, cx| EditorView::new(buffer, cx));
    let view = window.root(cx).expect("root view");
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    (view, visual)
}

fn text<V: InputHost>(view: &Entity<V>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.input().buffer().read(cx).text().to_owned()
    })
}

/// What `paint` hands the platform: the host and the bounds it was drawn in.
fn adapter<V: InputHost>(view: &Entity<V>, cx: &mut VisualTestContext) -> InputAdapter<V> {
    let bounds = view.read_with(cx, |view, _| view.input().last_bounds());
    InputAdapter::new(bounds.expect("the host is drawn"), view.clone())
}

fn platform<V: InputHost, R>(
    view: &Entity<V>,
    visual: &mut VisualTestContext,
    f: impl FnOnce(&mut InputAdapter<V>, &mut Window, &mut gpui::App) -> R,
) -> R {
    let mut adapter = adapter(view, visual);
    visual.update(|window, cx| f(&mut adapter, window, cx))
}

#[gpui::test]
fn text_for_range_counts_in_utf16(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "h😀llo");
    let mut actual = None;

    let text = platform(&view, &mut visual, |input, window, cx| {
        input.text_for_range(1..4, &mut actual, window, cx)
    });

    assert_eq!(
        text.as_deref(),
        Some("😀l"),
        "the emoji is two UTF-16 units"
    );
    assert_eq!(actual, Some(1..4));
}

#[gpui::test]
fn selected_text_range_is_in_utf16_and_keeps_its_direction(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "héllo");
    visual.simulate_keystrokes("right shift-left shift-left");

    let selection = platform(&view, &mut visual, |input, window, cx| {
        input.selected_text_range(false, window, cx)
    })
    .map(|selection| (selection.range, selection.reversed));

    assert_eq!(selection, Some((3..5, true)), "é is one UTF-16 unit");
}

#[gpui::test]
fn composing_marks_the_text_in_utf16_until_unmarked(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "é");
    visual.simulate_keystrokes("right");

    platform(&view, &mut visual, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "^", None, window, cx);
    });
    let marked = platform(&view, &mut visual, |input, window, cx| {
        input.marked_text_range(window, cx)
    });
    assert_eq!(marked, Some(1..2), "é is one UTF-16 unit");

    platform(&view, &mut visual, |input, window, cx| {
        input.unmark_text(window, cx);
    });
    let marked = platform(&view, &mut visual, |input, window, cx| {
        input.marked_text_range(window, cx)
    });
    assert_eq!(marked, None);
}

#[gpui::test]
fn a_single_line_input_turns_an_inserted_newline_into_a_space(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");

    platform(&view, &mut visual, |input, window, cx| {
        input.replace_text_in_range(None, "one\ntwo", window, cx);
    });

    assert_eq!(text(&view, &mut visual), "one two");
}

#[gpui::test]
fn a_single_line_input_turns_a_marked_newline_into_a_space(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");

    platform(&view, &mut visual, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "one\ntwo", None, window, cx);
    });

    assert_eq!(text(&view, &mut visual), "one two");
}

#[gpui::test]
fn a_single_line_input_turns_every_control_char_into_a_space(cx: &mut TestAppContext) {
    let (inserted, mut inserted_cx) = field(cx, "");
    platform(&inserted, &mut inserted_cx, |input, window, cx| {
        input.replace_text_in_range(None, "a\r\nb\tc", window, cx);
    });
    assert_eq!(text(&inserted, &mut inserted_cx), "a  b c");

    let (marked, mut marked_cx) = field(cx, "");
    platform(&marked, &mut marked_cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "a\r\nb\tc", None, window, cx);
    });
    assert_eq!(text(&marked, &mut marked_cx), "a  b c");
}

#[gpui::test]
fn a_multiline_input_keeps_an_inserted_newline(cx: &mut TestAppContext) {
    let (view, mut visual) = editor(cx, "");

    platform(&view, &mut visual, |input, window, cx| {
        input.replace_text_in_range(None, "one\ntwo", window, cx);
    });

    assert_eq!(text(&view, &mut visual), "one\ntwo");
}

#[gpui::test]
fn a_multiline_input_turns_every_line_ending_into_lf(cx: &mut TestAppContext) {
    let (inserted, mut inserted_cx) = editor(cx, "");
    platform(&inserted, &mut inserted_cx, |input, window, cx| {
        input.replace_text_in_range(None, "a\r\nb\rc", window, cx);
    });
    assert_eq!(text(&inserted, &mut inserted_cx), "a\nb\nc");

    let (marked, mut marked_cx) = editor(cx, "");
    platform(&marked, &mut marked_cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "a\r\nb\rc", None, window, cx);
    });
    assert_eq!(text(&marked, &mut marked_cx), "a\nb\nc");
}

#[gpui::test]
fn the_adapter_prefers_the_ime_for_printable_keys_on_both_hosts(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");

    let prefers = platform(&view, &mut visual, |input, window, cx| {
        input.prefers_ime_for_printable_keys(window, cx)
    });

    assert!(prefers, "NameField");

    let (view, mut visual) = editor(cx, "plan");
    let prefers = platform(&view, &mut visual, |input, window, cx| {
        input.prefers_ime_for_printable_keys(window, cx)
    });
    assert!(prefers, "EditorView");
}

#[gpui::test]
fn a_caret_move_fires_no_buffer_observer(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    let buffer = view.read_with(&visual, |view, _| view.input().buffer().clone());
    let fired = Rc::new(Cell::new(0));
    let _guard = visual.update(|_, cx| {
        let fired = Rc::clone(&fired);
        cx.observe(&buffer, move |_, _| fired.set(fired.get() + 1))
    });

    visual.simulate_keystrokes("left right shift-left cmd-a");

    assert_eq!(fired.get(), 0);
}

/// Hosts the field away from the window origin, as the picker does.
struct Inset(Entity<NameField>);

impl Render for Inset {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pl(px(40.)).pt(px(20.)).child(self.0.clone())
    }
}

fn inset_field(cx: &mut TestAppContext, initial: &str) -> (Entity<NameField>, VisualTestContext) {
    let initial = initial.to_owned();
    let (inset, visual) =
        cx.add_window_view(move |_, cx| Inset(cx.new(|cx| NameField::new(&initial, cx))));
    let visual = visual.clone();
    let view = inset.read_with(&visual, |inset, _| inset.0.clone());
    visual.run_until_parked();
    (view, visual)
}

#[gpui::test]
fn the_ime_rectangle_hit_tests_back_to_its_range(cx: &mut TestAppContext) {
    let (view, mut visual) = inset_field(cx, "héllo world");
    let element = view
        .read_with(&visual, |view, _| view.input().last_bounds())
        .expect("the field is drawn");
    assert_eq!(element.origin, point(px(40.), px(20.)));

    platform(&view, &mut visual, |input, window, cx| {
        let rect = input
            .bounds_for_range(1..3, window, cx)
            .expect("the field is laid out");
        assert_eq!(rect.top(), element.top());
        assert!(rect.size.height > px(0.));
        assert!(rect.left() > element.left(), "h precedes the range");

        let inside = px(1.);
        let y = rect.center().y;
        let start = input.character_index_for_point(point(rect.left() + inside, y), window, cx);
        let end = input.character_index_for_point(point(rect.right() - inside, y), window, cx);
        assert_eq!((start, end), (Some(1), Some(3)));
    });
}

/// `last_bounds` is already scrolled: a hit test that subtracted the offset again would land far below.
#[gpui::test]
fn a_hit_test_while_scrolled_uses_the_painted_bounds(cx: &mut TestAppContext) {
    let (view, mut visual) = editor(cx, &"line\n".repeat(300));
    let line_120 = ByteOffset::new(120 * "line\n".len());
    view.update(&mut visual, |view, cx| view.scroll_to_top(line_120, cx));
    visual.run_until_parked();

    let half_a_line = px(14.);
    let hit = view.read_with(&visual, |view, _| {
        let (layout, local) = view.input().hit_test(point(px(0.), half_a_line))?;
        Some(layout.offset_for_position(local))
    });

    assert_eq!(hit, Some(line_120));
}
