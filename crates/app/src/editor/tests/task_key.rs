//! `⌘L` cycles the caret's line through the task states (task boxes spec, D6).

use gpui::{TestAppContext, VisualTestContext, WindowHandle};

use super::dispatching;
use crate::editor::EditorView;

type Handle = WindowHandle<EditorView>;

fn text(window: &Handle, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

fn caret(window: &Handle, cx: &mut VisualTestContext) -> usize {
    window.update(cx, |view, _, _| view.cursor().get()).unwrap()
}

/// The caret stays before the line's text, so typing lands there; one `⌘Z` undoes the key.
#[gpui::test]
fn cmd_l_on_a_plain_line_makes_it_a_task(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "a\nb");
    cx.simulate_keystrokes("cmd-up cmd-l");
    assert_eq!(text(&window, &mut cx), "- [ ] a\nb");
    assert_eq!(caret(&window, &mut cx), 6);

    cx.simulate_keystrokes("z");
    assert_eq!(text(&window, &mut cx), "- [ ] za\nb");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&window, &mut cx), "- [ ] a\nb");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&window, &mut cx), "a\nb");
}

#[gpui::test]
fn cmd_l_on_a_task_ticks_it_then_clears_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "x\n- [ ] a");
    cx.simulate_keystrokes("cmd-down cmd-l");
    assert_eq!(text(&window, &mut cx), "x\n- [x] a");
    assert_eq!(caret(&window, &mut cx), 9, "the caret keeps its place");

    cx.simulate_keystrokes("cmd-l");
    assert_eq!(text(&window, &mut cx), "x\n- [ ] a");
    assert_eq!(caret(&window, &mut cx), 9);
}
