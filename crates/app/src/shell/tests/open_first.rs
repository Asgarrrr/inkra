//! The window opens before the vault is read (open-first spec, O6).

use std::path::Path;
use std::sync::Arc;

use gpui::{
    Bounds, Focusable, TestAppContext, VisualTestContext, WindowBounds, WindowHandle, WindowOptions,
};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Vault};

use crate::bootstrap::{Launch, root};
use crate::keymap::key_bindings;
use crate::picker::runs::NOTES_CHANGED_REFRESH;
use crate::shell::AppShell;
use crate::test_support::vault;

/// A shell over a vault nothing has read yet, as `bootstrap::launch` builds it.
fn unscanned(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::clone(fs) as Arc<dyn Fs>;
    // `open_window`, not `add_window`: `root` hands back an already-built `Entity<AppShell>`.
    let window = cx
        .update(|cx| {
            let bounds = Bounds::maximized(None, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    root(
                        fs,
                        Launch::Vault(Vault::new(Path::new("/vault")), None),
                        window,
                        cx,
                    )
                },
            )
        })
        .expect("open the window");
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("the shell is open");
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

fn rows(window: WindowHandle<AppShell>, visual: &mut VisualTestContext) -> Vec<String> {
    window
        .update(visual, |shell, _, cx| shell.tree_row_names(cx))
        .expect("the shell is open")
}

fn shown(
    window: WindowHandle<AppShell>,
    visual: &mut VisualTestContext,
) -> (Option<String>, usize) {
    window
        .update(visual, |shell, _, cx| shell.quick_open_state(cx))
        .expect("the shell is open")
        .expect("quick open is open")
}

#[gpui::test]
fn the_window_draws_empty_then_the_root_level_then_the_rest(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "f/g/c.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual) = unscanned(cx, &fs);

    assert_eq!(fs.read_dirs(Path::new("/vault")), 0);
    assert_eq!(
        rows(window, &mut visual),
        Vec::<String>::new(),
        "nothing read before the first frame"
    );
    visual.run_until_parked();
    assert_eq!(
        rows(window, &mut visual),
        ["f", "a"],
        "the root level, folders closed"
    );

    gate.release();
    visual.run_until_parked();
    assert_eq!(
        rows(window, &mut visual),
        ["f", "a"],
        "nothing moves: f stays closed"
    );
    window
        .update(&mut visual, |shell, _, cx| shell.toggle_tree_row("f", cx))
        .expect("the shell is open");
    assert_eq!(rows(window, &mut visual), ["f", "g", "b", "a"]);
}

#[gpui::test]
fn a_folder_expanded_during_the_scan_fills_in_when_the_scan_applies(cx: &mut TestAppContext) {
    let fs = vault(&["f/b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual) = unscanned(cx, &fs);
    visual.run_until_parked();

    window
        .update(&mut visual, |shell, _, cx| shell.toggle_tree_row("f", cx))
        .expect("the shell is open");
    assert_eq!(rows(window, &mut visual), ["f"]);
    gate.release();
    visual.run_until_parked();

    assert_eq!(rows(window, &mut visual), ["f", "b"]);
}

#[gpui::test]
fn cmd_p_during_the_scan_says_so_then_lists(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual) = unscanned(cx, &fs);
    visual.run_until_parked();

    // An empty query lists recent notes only; a query lists matches.
    visual.simulate_keystrokes("cmd-p b");
    visual.run_until_parked();
    assert_eq!(
        shown(window, &mut visual),
        (Some("Scanning the vault…".into()), 0)
    );

    gate.release();
    visual.run_until_parked();
    visual.executor().advance_clock(NOTES_CHANGED_REFRESH);
    visual.run_until_parked();
    assert_eq!(shown(window, &mut visual).1, 1, "f/b, after one build");
}

#[gpui::test]
fn an_unreadable_root_during_the_scan_releases_quick_open(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    fs.make_unreadable("/vault");
    let (window, mut visual) = unscanned(cx, &fs);
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-p");
    visual.executor().advance_clock(NOTES_CHANGED_REFRESH);
    visual.run_until_parked();

    assert_eq!(
        shown(window, &mut visual),
        (Some("No recent notes".into()), 0),
        "never waits forever"
    );
}

/// `enter` on "Scanning the vault…" confirms nothing, and nothing after the first build either.
#[gpui::test]
fn enter_during_the_scan_opens_nothing_once_it_ends(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual) = unscanned(cx, &fs);
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-p b enter");
    gate.release();
    visual.run_until_parked();
    visual.executor().advance_clock(NOTES_CHANGED_REFRESH);
    visual.run_until_parked();

    let (open, showing) = window
        .update(&mut visual, |shell, _, _| {
            (shell.modal.is_some(), shell.showing())
        })
        .expect("the shell is open");
    assert!(open, "the picker is still open");
    assert_eq!(showing, None, "no note was activated");
}
