use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{Entity, Focusable, Subscription, TestAppContext, VisualTestContext, point, px};
use inkra_markdown::Markdown;
use inkra_text::ByteOffset;

use super::{Cancelled, Confirmed, Edited, NameField, shape_document};
use crate::keymap::key_bindings;
use crate::text_input::InputHost;

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
fn a_one_line_layout_hit_tests_the_field_like_the_editor(cx: &mut TestAppContext) {
    let (view, mut visual) = field(cx, "hello world");

    visual.update(|window, cx| {
        view.update(cx, |field, cx| {
            let marked = field.input().state().marked_range();
            let style = window.text_style();
            let layout = shape_document(
                field.input().buffer().read(cx),
                &Markdown::default(),
                &field.input().state().selection().range(),
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
