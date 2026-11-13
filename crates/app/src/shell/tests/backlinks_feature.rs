//! Backlinks as a feature: switched off and on, restored without it, idle while hidden (feature
//! registry spec, D6, D9, D11).

use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::Side;
use inkra_vault::Vault;
use inkra_vault::mem_fs::MemFs;

use super::dock_menu::side_of;
use super::dragging::drag;
use super::pane_features::{SETTINGS, right_dock, switch};
use super::restore_lands::{backlinks_left, launch_as, save_docks, vault};
use super::session_writes::SESSION;
use super::splits::check_layout;
use super::tab_bar::click;
use super::{keys, open, shell_focused};
use crate::backlinks::{BACKLINKS, BacklinksPanel};
use crate::bootstrap::Launch;
use crate::feature::{BUILTIN, Registry};
use crate::settings;
use crate::shell::docks::MoveFilesToOtherDock;
use crate::shell::{AppShell, FILES};
use crate::test_support;

/// `a.md` links to `b.md`; the shell's root focused, backlinks active.
fn linked_shell(
    cx: &mut TestAppContext,
) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha [[b]]");
    fs.insert_file("/vault/b.md", "beta");
    let (window, _notes, cx) = shell_focused(cx, &fs);
    (fs, window, cx)
}

fn open_named(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, name: &str) {
    let notes = window
        .update(cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    let id = test_support::entry_id(&notes, cx, name);
    open(window, cx, id);
}

fn panel(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Entity<BacklinksPanel> {
    window
        .update(cx, |shell, _, _| shell.backlinks_panel())
        .unwrap()
        .expect("backlinks is active")
}

fn sources(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<String> {
    panel(window, cx).read_with(cx, |panel, _| panel.source_names())
}

/// Backlinks is in no tab and registers no view, and the layout holds.
fn assert_gone(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, _, cx| {
            assert_eq!(shell.tab_of(BACKLINKS), None);
            assert!(shell.backlinks_panel().is_none());
            shell.check_layout(cx);
        })
        .unwrap();
}

#[gpui::test]
fn disable_backlinks_while_alone_in_its_group(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    click(&mut cx, "dock-right-group");

    switch(&fs, "backlinks", false, &mut cx);

    assert_gone(&window, &mut cx);
    window
        .update(&mut cx, |shell, window, cx| {
            assert!(shell.layout.dock_groups(Side::Right).is_empty());
            assert!(shell.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn closed_backlinks_tab_does_not_reopen_after_disable(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    open_named(&window, &mut cx, "a");
    drag(&mut cx, "dock-icon-backlinks", "group-strip");
    keys(&mut cx, "cmd-w");

    switch(&fs, "backlinks", false, &mut cx);
    keys(&mut cx, "cmd-shift-t");

    assert_gone(&window, &mut cx);
}

#[gpui::test]
fn restart_with_backlinks_disabled_restores_without_it(cx: &mut TestAppContext) {
    let fs = vault();
    save_docks(&fs, backlinks_left());
    fs.insert_file(SETTINGS, r#"{"features":{"backlinks":false}}"#);
    let switches = block_on(settings::read(
        &*fs,
        Path::new("/vault/.inkra"),
        &mut Vec::new(),
    ))
    .features
    .unwrap_or_default();
    let launched = Launch::Vault(Vault::new(Path::new("/vault")), Some(SESSION.into()), None);

    let (window, mut cx) = launch_as(cx, &fs, launched, Registry::new(BUILTIN, &switches));
    cx.run_until_parked();

    assert_gone(&window, &mut cx);
    assert_eq!(side_of(&window, &mut cx, FILES), Some(Side::Left));
}

#[gpui::test]
fn reenabled_backlinks_returns_to_its_dock(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    cx.dispatch_action(MoveFilesToOtherDock);
    cx.run_until_parked();
    switch(&fs, "backlinks", false, &mut cx);

    switch(&fs, "backlinks", true, &mut cx);

    assert_eq!(
        right_dock(window, &mut cx),
        (vec![FILES, BACKLINKS], Some(FILES))
    );
    check_layout(&window, &mut cx);
}

#[gpui::test]
fn disabled_backlinks_panel_is_released(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    let weak = panel(&window, &mut cx).downgrade();

    switch(&fs, "backlinks", false, &mut cx);

    assert!(weak.upgrade().is_none());
}

#[gpui::test]
fn backlinks_shown_after_hidden_open_lists_the_rows(cx: &mut TestAppContext) {
    let (_fs, window, mut cx) = linked_shell(cx);
    keys(&mut cx, "cmd-|");
    open_named(&window, &mut cx, "b");

    keys(&mut cx, "cmd-|");

    assert_eq!(sources(&window, &mut cx), ["a"]);
}

#[gpui::test]
fn hidden_backlinks_rebuild_zero_times_then_once_on_show(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    open_named(&window, &mut cx, "b");
    keys(&mut cx, "cmd-|");
    let rebuilds =
        |cx: &mut VisualTestContext| panel(&window, cx).read_with(cx, |p, _| p.rebuilds());
    let before = rebuilds(&mut cx);

    fs.insert_file("/vault/c.md", "gamma [[b]]");
    cx.run_until_parked();
    assert_eq!(rebuilds(&mut cx), before);

    keys(&mut cx, "cmd-|");

    assert_eq!(rebuilds(&mut cx), before + 1);
    assert_eq!(sources(&window, &mut cx), ["a", "c"]);
}

#[gpui::test]
fn palette_drops_backlinks_move_when_disabled(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = linked_shell(cx);
    switch(&fs, "backlinks", false, &mut cx);

    keys(&mut cx, "cmd-shift-p");

    let names = window
        .update(&mut cx, |shell, _, cx| shell.palette_names(cx))
        .unwrap()
        .expect("the palette is open");
    assert!(names.iter().any(|name| name == "Move files to other dock"));
    assert!(
        !names
            .iter()
            .any(|name| name == "Move backlinks to other dock")
    );
}
