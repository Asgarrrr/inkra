use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Bounds, Focusable, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowBounds,
    WindowHandle, WindowOptions, px, size,
};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::{
    AppShell, ab_focused, check, editor, focus_tree, keys, no_vault, open, showing, tab_notes,
};
use crate::bootstrap::{Launch, root};

fn titles(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Vec<gpui::SharedString> {
    window
        .update(cx, |shell, _, cx| {
            shell.tab_titles(shell.layout.last_center(), cx)
        })
        .unwrap()
}

pub(super) fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("drawn");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// Whether the active tab's content (its editor, or the root on an empty tab) and the tree hold focus.
fn focus(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> (bool, bool) {
    window
        .update(cx, |shell, window, cx| {
            (
                shell.focus_handle(cx).is_focused(window),
                shell.tree_focus_handle(cx).is_focused(window),
            )
        })
        .unwrap()
}

pub(super) fn press(cx: &mut VisualTestContext, selector: &'static str, button: MouseButton) {
    let at = cx.debug_bounds(selector).expect("drawn").center();
    cx.simulate_mouse_down(at, button, Modifiers::none());
    cx.simulate_mouse_up(at, button, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn titles_are_note_names_and_new_tab_for_an_empty_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    assert_eq!(titles(&window, &mut cx), ["a", "New tab", "b"]);
}

#[gpui::test]
fn single_file_mode_titles_the_tab_with_the_file_name(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", "alpha");
    let loaded = futures::executor::block_on(fs.load(Path::new("/notes/a.md"))).unwrap();
    let launch = Launch::Note(PathBuf::from("/notes/a.md"), loaded.text, loaded.stamp);
    let window = cx
        .update(|cx| {
            let bounds = Bounds::maximized(None, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| root(fs as Arc<dyn Fs>, launch, window, cx),
            )
        })
        .unwrap();

    assert_eq!(titles(&window, cx), ["a.md"]);
}

#[gpui::test]
fn renaming_a_note_in_an_inactive_tab_renames_its_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    let (shell, notes) = window
        .update(&mut cx, |shell, _, cx| (cx.entity(), shell.notes.clone()))
        .unwrap();
    let notified = Rc::new(Cell::new(false));
    let set = Rc::clone(&notified);
    let _subscription = cx.update(|_, cx| cx.observe(&shell, move |_, _| set.set(true)));

    let renamed = notes.update(&mut cx, |notes, cx| notes.rename(a, "plan", cx));
    cx.run_until_parked();

    assert!(futures::executor::block_on(renamed));
    assert_eq!(titles(&window, &mut cx), ["plan", "b"]);
    assert!(notified.get(), "the bar must redraw with the new title");
}

#[gpui::test]
fn clicking_a_tab_activates_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    click(&mut cx, "tab-0");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 0));
    let text = editor(&window, &mut cx).update(&mut cx, |editor, cx| {
        editor.buffer().read(cx).text().to_owned()
    });
    assert_eq!(text, "alpha");
    assert_eq!(focus(&window, &mut cx), (true, false));
}

#[gpui::test]
fn clicking_x_closes_that_tab_and_keeps_the_active_one(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    click(&mut cx, "tab-close-0");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b)], 0));
    let showing = window
        .update(&mut cx, |shell, _, _| shell.showing())
        .unwrap();
    assert_eq!(showing, Some(b));
    assert_eq!(focus(&window, &mut cx), (true, false));
}

#[gpui::test]
fn clicking_the_active_tab_focuses_its_editor(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);

    click(&mut cx, "tab-0");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(focus(&window, &mut cx), (true, false));
}

#[gpui::test]
fn closing_an_inactive_tab_with_its_x_focuses_the_active_editor(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    focus_tree(&window, &mut cx);

    click(&mut cx, "tab-close-0");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b)], 0));
    assert_eq!(focus(&window, &mut cx), (true, false));
}

#[gpui::test]
fn a_middle_click_on_an_inactive_tab_focuses_the_active_editor(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    focus_tree(&window, &mut cx);

    press(&mut cx, "tab-0", MouseButton::Middle);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b)], 0));
    assert_eq!(focus(&window, &mut cx), (true, false));
}

#[gpui::test]
fn clicking_a_tab_on_an_empty_tab_focuses_the_root(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    focus_tree(&window, &mut cx);

    click(&mut cx, "tab-0");

    assert_eq!(focus(&window, &mut cx), (true, false));
    keys(&mut cx, "cmd-p");
    let modal = window
        .update(&mut cx, |shell, _, _| shell.modal.is_some())
        .unwrap();
    assert!(modal, "cmd-p reaches the shell");
}

#[gpui::test]
fn the_backlinks_column_shows_for_an_empty_tab_in_a_vault_only(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(
        cx.debug_bounds("dock-right").is_some(),
        "a vault, an empty tab"
    );

    let (_window, mut cx) = no_vault(&mut cx);
    assert!(cx.debug_bounds("dock-right").is_none(), "no vault");
}

/// 30 tabs cannot fit an 800 px window; `⌘9` must bring the last one into the bar.
#[gpui::test]
fn a_tab_activated_past_the_bar_edge_scrolls_into_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(800.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    let bar = cx.debug_bounds("tab-bar").expect("the bar is drawn");
    let last = cx.debug_bounds("tab-29").expect("the last tab is drawn");
    assert!(
        last.left() >= bar.right(),
        "the last tab starts past the edge"
    );

    cx.simulate_keystrokes("cmd-9");
    cx.run_until_parked();

    let bar = cx.debug_bounds("tab-bar").unwrap();
    let last = cx.debug_bounds("tab-29").unwrap();
    assert!(
        last.left() >= bar.left() && last.right() <= bar.right(),
        "tab-29 at {last:?} is outside the bar at {bar:?}"
    );
}

/// Closing a tab left of the active one shifts it; the bar must scroll it back into view.
#[gpui::test]
fn closing_a_tab_before_the_active_one_keeps_it_in_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(800.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    cx.simulate_keystrokes("cmd-9");
    // Walking left leaves tab 20 at the bar's left edge, the scroll short of its end.
    for _ in 20..29 {
        cx.simulate_keystrokes("ctrl-shift-tab");
    }
    cx.run_until_parked();
    assert_eq!(tab_notes(&window, &mut cx).1, 20);
    let bar = cx.debug_bounds("tab-bar").unwrap();
    let active = cx.debug_bounds("tab-20").unwrap();
    assert!(active.left() >= bar.left() && active.left() < bar.left() + px(1.));

    window
        .update(&mut cx, |shell, window, cx| {
            let first = shell.center().tabs.id_at(0).unwrap();
            shell.close_tab(shell.layout.last_center(), first, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx).1, 19);
    let bar = cx.debug_bounds("tab-bar").unwrap();
    let active = cx.debug_bounds("tab-19").unwrap();
    assert!(
        active.left() >= bar.left() && active.right() <= bar.right(),
        "the active tab at {active:?} left the bar at {bar:?}"
    );
}

/// As `column.rs`: the bar's min-content width must not push the note column past the window.
#[gpui::test]
fn many_tabs_never_widen_the_note_column(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();

    let backlinks = cx.debug_bounds("dock-right").unwrap();
    let text = editor(&window, &mut cx).read_with(&cx, |view, _| view.text_bounds().unwrap());
    assert!(
        text.right() <= backlinks.left(),
        "the column ends at {:?}",
        text.right()
    );
    assert!(
        backlinks.right() <= px(700.),
        "the backlinks column is pushed out"
    );
}

#[gpui::test]
fn a_middle_click_closes_the_tab_under_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");

    press(&mut cx, "tab-0", MouseButton::Middle);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), None], 1));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_right_click_on_a_tab_closes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");

    press(&mut cx, "tab-0", MouseButton::Right);

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), None], 0)
    );
}

/// The most recent tab is `b`'s, two to the right; the neighbour is empty.
#[gpui::test]
fn a_middle_click_on_the_active_tab_shows_the_most_recent_one(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-1");

    press(&mut cx, "tab-0", MouseButton::Middle);

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, Some(b)], 1));
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_middle_click_on_the_x_closes_its_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");

    press(&mut cx, "tab-close-0", MouseButton::Middle);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), None], 1));
    check(&window, &mut cx);
}

/// The picker's overlay covers the bar.
#[gpui::test]
fn a_middle_click_on_a_tab_under_cmd_p_closes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-p");

    press(&mut cx, "tab-0", MouseButton::Middle);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 1));
}
