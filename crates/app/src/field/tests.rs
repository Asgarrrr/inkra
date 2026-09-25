use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext, Context, Entity, EntityInputHandler, IntoElement, ParentElement, Render, Styled,
    Subscription, TestAppContext, VisualTestContext, Window, div, point, px,
};
use inkra_markdown::Markdown;
use inkra_text::ByteOffset;

use super::{Cancelled, Confirmed, Edited, NameField, shape_document};
use crate::keymap::key_bindings;

fn field(cx: &mut TestAppContext, initial: &str) -> (Entity<NameField>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let initial = initial.to_owned();
    let window = cx.add_window(move |_, cx| NameField::new(&initial, cx));
    window
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle, cx);
        })
        .unwrap();
    let view = window.root(cx).expect("root view");
    (view, VisualTestContext::from_window(window.into(), cx))
}

fn text(view: &Entity<NameField>, cx: &mut TestAppContext) -> String {
    view.read_with(cx, NameField::text)
}

fn watch_confirm(
    view: &Entity<NameField>,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<String>>>, Subscription) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let guard = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.subscribe(view, move |_, event: &Confirmed, _| {
            seen.borrow_mut().push(event.0.clone());
        })
    });
    (seen, guard)
}

#[gpui::test]
fn it_opens_with_the_initial_name_selected_so_typing_replaces_it(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "old name");
    visual.simulate_keystrokes("n e w");

    assert_eq!(text(&view, cx), "new");
}

#[gpui::test]
fn backspace_deletes_in_the_field(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("right backspace");

    assert_eq!(text(&view, cx), "pla");
}

#[gpui::test]
fn the_arrows_move_the_caret_rather_than_leaving_the_field(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "ab");
    visual.simulate_keystrokes("right left X");

    assert_eq!(text(&view, cx), "aXb");
}

#[gpui::test]
fn cmd_left_reaches_the_start(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("right cmd-left X");

    assert_eq!(text(&view, cx), "Xplan");
}

#[gpui::test]
fn cmd_right_reaches_the_end(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("left cmd-right X");

    assert_eq!(text(&view, cx), "planX");
}

#[gpui::test]
fn ctrl_a_reaches_the_start(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("right ctrl-a X");

    assert_eq!(text(&view, cx), "Xplan");
}

#[gpui::test]
fn ctrl_e_reaches_the_end(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("left ctrl-e X");

    assert_eq!(text(&view, cx), "planX");
}

#[gpui::test]
fn select_all_then_typing_replaces_everything(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("right cmd-a Z");

    assert_eq!(text(&view, cx), "Z");
}

#[gpui::test]
fn enter_confirms_and_carries_the_text(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "old");
    let (seen, _guard) = watch_confirm(&view, cx);

    visual.simulate_keystrokes("n e w enter");

    assert_eq!(*seen.borrow(), ["new".to_owned()]);
}

#[gpui::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "old");
    let cancelled = Rc::new(RefCell::new(0));
    let _guard = cx.update(|cx| {
        let cancelled = Rc::clone(&cancelled);
        cx.subscribe(&view, move |_, _: &Cancelled, _| {
            *cancelled.borrow_mut() += 1;
        })
    });

    visual.simulate_keystrokes("escape");

    assert_eq!(*cancelled.borrow(), 1);
}

#[gpui::test]
fn a_dead_key_composes_rather_than_typing_two_characters(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            field.replace_and_mark_text_in_range(None, "^", None, window, cx);
        });
    });
    assert_eq!(text(&view, cx), "^", "the accent shows while composing");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            field.replace_text_in_range(None, "ê", window, cx);
        });
    });

    assert_eq!(text(&view, cx), "ê");
}

#[gpui::test]
fn a_pasted_newline_does_not_give_the_field_a_second_line(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            field.replace_text_in_range(None, "one\ntwo", window, cx);
        });
    });

    assert_eq!(text(&view, cx), "one two");
}

#[gpui::test]
fn a_marked_newline_does_not_give_the_field_a_second_line(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            field.replace_and_mark_text_in_range(None, "one\ntwo", None, window, cx);
        });
    });

    assert_eq!(text(&view, cx), "one two");
}

#[gpui::test]
fn a_one_line_layout_hit_tests_the_field_like_the_editor(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "hello world");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            let marked = field.state.marked_range();
            let style = window.text_style();
            let layout = shape_document(
                field.buffer.read(cx),
                &Markdown::default(),
                &field.state.selection().range(),
                marked.as_ref(),
                &style,
                None,
                window,
            );

            assert_eq!(layout.row_count(), 1);
            assert_eq!(layout.top_for_row(0), px(0.));

            let mid = ByteOffset(5); // the boundary between "hello" and " world"
            let x = layout.x_for_offset(mid);
            assert_eq!(layout.offset_for_position(point(x, px(0.))), mid);
        });
    });
}

fn watch_edits(
    view: &Entity<NameField>,
    cx: &mut TestAppContext,
) -> (Rc<Cell<usize>>, Subscription) {
    let edits = Rc::new(Cell::new(0));
    let guard = cx.update(|cx| {
        let edits = Rc::clone(&edits);
        cx.subscribe(view, move |_, _: &Edited, _| edits.set(edits.get() + 1))
    });
    (edits, guard)
}

#[gpui::test]
fn each_edit_emits_one_edited(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");
    let (edits, _guard) = watch_edits(&view, cx);

    visual.simulate_keystrokes("a b backspace");

    assert_eq!(text(&view, cx), "a");
    assert_eq!(edits.get(), 3);
}

#[gpui::test]
fn a_key_that_changes_nothing_emits_no_edited(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "");
    let (edits, _guard) = watch_edits(&view, cx);

    visual.simulate_keystrokes("backspace delete left right");

    assert_eq!(edits.get(), 0);
}

#[gpui::test]
fn delete_removes_the_character_after_the_caret(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("left delete");

    assert_eq!(text(&view, cx), "lan");
}

#[gpui::test]
fn shift_left_extends_the_selection_leftward(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("right shift-left shift-left X");

    assert_eq!(text(&view, cx), "plX");
}

#[gpui::test]
fn shift_right_extends_the_selection_rightward(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "plan");
    visual.simulate_keystrokes("left shift-right shift-right X");

    assert_eq!(text(&view, cx), "Xan");
}

fn handle<R>(
    view: &Entity<NameField>,
    visual: &mut VisualTestContext,
    f: impl FnOnce(&mut NameField, &mut Window, &mut Context<NameField>) -> R,
) -> R {
    visual.update(|window, cx| view.update(cx, |field, cx| f(field, window, cx)))
}

#[gpui::test]
fn text_for_range_counts_in_utf16(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "h😀llo");
    let mut actual = None;

    let text = handle(&view, &mut visual, |field, window, cx| {
        field.text_for_range(1..4, &mut actual, window, cx)
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

    let selection = handle(&view, &mut visual, |field, window, cx| {
        field.selected_text_range(false, window, cx)
    })
    .map(|selection| (selection.range, selection.reversed));

    assert_eq!(selection, Some((3..5, true)), "é is one UTF-16 unit");
}

#[gpui::test]
fn composing_marks_the_text_in_utf16_until_unmarked(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "é");
    visual.simulate_keystrokes("right");

    handle(&view, &mut visual, |field, window, cx| {
        field.replace_and_mark_text_in_range(None, "^", None, window, cx);
    });
    let marked = handle(&view, &mut visual, |field, window, cx| {
        field.marked_text_range(window, cx)
    });
    assert_eq!(marked, Some(1..2), "é is one UTF-16 unit");

    handle(&view, &mut visual, |field, window, cx| {
        field.unmark_text(window, cx);
    });
    let marked = handle(&view, &mut visual, |field, window, cx| {
        field.marked_text_range(window, cx)
    });
    assert_eq!(marked, None);
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

    handle(&view, &mut visual, |field, window, cx| {
        let element = field.last_bounds.expect("the field is drawn");
        assert_eq!(element.origin, point(px(40.), px(20.)));

        let rect = field
            .bounds_for_range(1..3, element, window, cx)
            .expect("the field is laid out");
        assert_eq!(rect.top(), element.top());
        assert!(rect.size.height > px(0.));
        assert!(rect.left() > element.left(), "h precedes the range");

        let inside = px(1.);
        let y = rect.center().y;
        let start = field.character_index_for_point(point(rect.left() + inside, y), window, cx);
        let end = field.character_index_for_point(point(rect.right() - inside, y), window, cx);
        assert_eq!((start, end), (Some(1), Some(3)));
    });
}
