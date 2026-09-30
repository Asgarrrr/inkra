use gpui::TestAppContext;

use super::{ab_focused, editor, open, showing, tab_notes};

fn text_shown(window: &gpui::WindowHandle<super::AppShell>, cx: &mut TestAppContext) -> String {
    editor(window, cx).update(cx, |editor, cx| editor.buffer().read(cx).text().to_owned())
}

#[gpui::test]
fn cmd_t_opens_an_empty_tab_after_the_active_one(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 0));

    cx.simulate_keystrokes("cmd-t");

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), None, Some(b)], 1)
    );
    window
        .update(&mut cx, |shell, window, _| {
            assert!(shell.editor.is_none(), "an empty tab has no editor");
            assert!(
                shell.focus_handle.is_focused(window),
                "the root keeps the keys"
            );
        })
        .unwrap();
    let current = window
        .update(&mut cx, |shell, _, cx| shell.notes.read(cx).current())
        .unwrap();
    assert_eq!(current, None);
}

#[gpui::test]
fn ctrl_tab_and_ctrl_shift_tab_switch_and_wrap(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    for (keys, active, note) in [
        ("ctrl-tab", 0, a),
        ("ctrl-tab", 1, b),
        ("ctrl-shift-tab", 0, a),
        ("ctrl-shift-tab", 1, b),
    ] {
        cx.simulate_keystrokes(keys);
        assert_eq!(tab_notes(&window, &mut cx).1, active, "{keys}");
        assert_eq!(showing(&window, &mut cx), Some(note), "{keys}");
        assert_eq!(
            text_shown(&window, &mut cx),
            if note == a { "alpha" } else { "beta" }
        );
    }
}

/// `⌘⇧]` as macOS reports it (`gpui_macos` events.rs:470-479).
#[gpui::test]
fn cmd_brace_right_and_left_switch_tabs(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    cx.simulate_keystrokes("cmd-}");
    assert_eq!(showing(&window, &mut cx), Some(a));
    cx.simulate_keystrokes("cmd-{");
    assert_eq!(showing(&window, &mut cx), Some(b));
}

/// Tabs `[a, empty, b]`, used empty, b, a: the MRU picks `b`, a neighbour the empty tab.
#[gpui::test]
fn cmd_w_closes_the_active_tab_and_activates_the_most_recent(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("ctrl-tab cmd-t ctrl-tab ctrl-tab");
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), None, Some(b)], 0)
    );

    cx.simulate_keystrokes("cmd-w");

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, Some(b)], 1));
    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(text_shown(&window, &mut cx), "beta");
}

#[gpui::test]
fn cmd_w_on_the_last_tab_leaves_one_empty_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-w");

    assert_eq!(tab_notes(&window, &mut cx), (vec![None], 0));
    let (editor, current) = window
        .update(&mut cx, |shell, _, cx| {
            (shell.editor.is_some(), shell.notes.read(cx).current())
        })
        .unwrap();
    assert!(!editor, "an empty tab has no editor");
    assert_eq!(current, None);
}

/// `a`, then `count - 1` empty tabs, each opened after the last.
fn tabs_after_a(
    cx: &mut TestAppContext,
    count: usize,
) -> (gpui::WindowHandle<super::AppShell>, gpui::VisualTestContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    for _ in 1..count {
        cx.simulate_keystrokes("cmd-t");
    }
    assert_eq!(tab_notes(&window, &mut cx).0.len(), count);
    (window, cx)
}

#[gpui::test]
fn cmd_n_activates_the_nth_tab(cx: &mut TestAppContext) {
    let (window, mut cx) = tabs_after_a(cx, 8);
    for n in [1, 8, 2, 7, 3, 6, 4, 5] {
        cx.simulate_keystrokes(&format!("cmd-{n}"));
        assert_eq!(tab_notes(&window, &mut cx).1, n - 1, "cmd-{n}");
    }
}

#[gpui::test]
fn cmd_n_past_the_last_tab_does_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = tabs_after_a(cx, 2);
    cx.simulate_keystrokes("cmd-1");
    cx.simulate_keystrokes("cmd-5");
    assert_eq!(tab_notes(&window, &mut cx).1, 0);
    assert!(showing(&window, &mut cx).is_some());
}

/// Ten tabs: `⌘9` is the last, not the ninth.
#[gpui::test]
fn cmd_9_activates_the_last_tab(cx: &mut TestAppContext) {
    let (window, mut cx) = tabs_after_a(cx, 10);
    cx.simulate_keystrokes("cmd-1");
    cx.simulate_keystrokes("cmd-9");
    assert_eq!(tab_notes(&window, &mut cx).1, 9);
}
