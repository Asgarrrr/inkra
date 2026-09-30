use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Bounds, Modifiers, TestAppContext, VisualTestContext, WindowBounds, WindowHandle,
    WindowOptions, px, size,
};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::{AppShell, ab_focused, editor, no_vault, open, tab_notes};
use crate::bootstrap::{Launch, root};

fn titles(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Vec<String> {
    window
        .update(cx, |shell, _, cx| shell.tab_titles(cx))
        .unwrap()
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("drawn");
    cx.simulate_click(bounds.center(), Modifiers::none());
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
                |_, cx| root(fs as Arc<dyn Fs>, launch, cx),
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
}

#[gpui::test]
fn the_backlinks_column_shows_for_an_empty_tab_in_a_vault_only(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(
        cx.debug_bounds("backlinks-column").is_some(),
        "a vault, an empty tab"
    );

    let (_window, mut cx) = no_vault(&mut cx);
    assert!(cx.debug_bounds("backlinks-column").is_none(), "no vault");
}

/// 30 tabs cannot fit a 700 px window; `⌘9` must bring the last one into the bar.
#[gpui::test]
fn a_tab_activated_past_the_bar_edge_scrolls_into_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
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

/// Closing a tab left of the active one shifts it left; the bar must scroll it back into view.
/// The × of a tab scrolled out of the bar cannot be clicked, so this calls its handler.
#[gpui::test]
fn closing_a_tab_before_the_active_one_keeps_it_in_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
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
            let first = shell.tabs.id_at(0).unwrap();
            shell.close_tab(first, window, cx);
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

    let bar = cx.debug_bounds("tab-bar").unwrap();
    let backlinks = cx.debug_bounds("backlinks-column").unwrap();
    assert!(
        bar.right() <= backlinks.left(),
        "the bar ends at {:?}",
        bar.right()
    );
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
