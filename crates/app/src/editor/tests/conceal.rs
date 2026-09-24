use gpui::TestAppContext;

use super::{at, dispatching};

fn shown(view: &crate::editor::EditorView, line: usize) -> String {
    view.last_layout.as_ref().unwrap().shown(line).to_string()
}

#[gpui::test]
fn syntax_is_hidden_away_from_the_caret(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a **b** c\nx");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| assert_eq!(shown(view, 0), "a b c"))
        .unwrap();
}

#[gpui::test]
fn the_caret_on_an_element_reveals_it_and_leaving_hides_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a **b** c\nx");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.cursor(), at(2));
            assert_eq!(shown(view, 0), "a **b** c");
        })
        .unwrap();
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(shown(view, 0), "a b c"))
        .unwrap();
}

#[gpui::test]
fn a_move_that_reveals_reshapes_before_the_next_frame(cx: &mut TestAppContext) {
    // A frame first: `last_layout` only exists once the element has prepainted.
    let (window, mut cx) = dispatching(cx, "a **b** c\nx");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, window, cx| {
            view.move_cursor(window, cx, |state, buffer| {
                state.move_to(buffer, at(3), false);
            });
            assert_eq!(shown(view, 0), "a **b** c");
        })
        .unwrap();
}

#[gpui::test]
fn end_lands_after_a_hidden_closing_delimiter(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a **b**\nx");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up end");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.cursor(), at(7));
            assert_eq!(shown(view, 0), "a **b**");
        })
        .unwrap();
}

#[gpui::test]
fn up_onto_a_concealed_line_lands_at_its_true_end(cx: &mut TestAppContext) {
    // Line 0 shows "bold text" (9 chars); line 1 has 15. Up from the end of line 1 aims past line 0's end.
    let (window, mut cx) = dispatching(cx, "**bold** text\nplain line here");
    cx.run_until_parked();
    cx.simulate_keystrokes("up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(13)))
        .unwrap();
}

#[gpui::test]
fn select_all_reveals_everything(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a **b** c\n# T");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(shown(view, 0), "a **b** c");
            // A heading's `# ` hangs left of the text instead (live-preview spec, D12).
            assert_eq!(shown(view, 1), "T");
            let layout = view.last_layout.as_ref().unwrap();
            assert!(layout.x_for_offset(at(10)) < gpui::px(0.));
        })
        .unwrap();
}

#[gpui::test]
fn a_wikilink_shows_its_alias(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "[[Page|alias]] x\ny");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| assert_eq!(shown(view, 0), "alias x"))
        .unwrap();
}

#[gpui::test]
fn a_bullet_shows_a_dot_unless_the_caret_is_on_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- a\nx");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| assert_eq!(shown(view, 0), "• a"))
        .unwrap();
    cx.simulate_keystrokes("cmd-up right right");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.cursor(), at(2));
            assert_eq!(shown(view, 0), "• a");
        })
        .unwrap();
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(shown(view, 0), "- a"))
        .unwrap();
}

#[gpui::test]
fn typing_inside_revealed_bold_keeps_it_revealed(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a **b** c\nx");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up right right right right right");
    cx.simulate_input("z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "a **bz** c\nx");
            assert_eq!(shown(view, 0), "a **bz** c");
        })
        .unwrap();
}
