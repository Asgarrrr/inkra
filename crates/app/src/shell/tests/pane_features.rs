//! A feature's pane in a real shell: dormant until shown, told when hidden, switched off and on
//! (feature registry spec, D5, D6, D9).

use std::sync::Arc;

use gpui::{Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{GroupId, Side, SplitDirection, ViewId};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::tab_bar::click;
use super::{open, open_vault_with};
use crate::feature::Registry;
use crate::feature::fakes::{self, PANE_FAKE_VIEW as PANE};
use crate::keymap::key_bindings;
use crate::settings::Switches;
use crate::shell::AppShell;
use crate::shell::docks::SplitView;
use crate::test_support;

const SETTINGS: &str = "/vault/.inkra/settings.json";

/// `/vault/a.md` behind a shell with the pane fake on, launched, its root focused.
fn pane_shell(cx: &mut TestAppContext) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let features = Registry::new(fakes::PANE_CATALOG, &Switches::default());
    let (window, _sidebar, _notes) = open_vault_with(cx, Arc::clone(&fs) as Arc<dyn Fs>, features);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(&mut visual, |shell, window, cx| {
            window.focus(&shell.focus_handle, cx);
        })
        .unwrap();
    visual.run_until_parked();
    (fs, window, visual)
}

fn switch(fs: &MemFs, on: bool, cx: &mut VisualTestContext) {
    fs.insert_file(SETTINGS, format!(r#"{{"features":{{"pane_fake":{on}}}}}"#));
    cx.run_until_parked();
}

fn focus_input(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |_, window, cx| {
            let (focus, _) = fakes::pane_input(cx).expect("the pane is active");
            window.focus(&focus, cx);
        })
        .unwrap();
    cx.run_until_parked();
}

/// The right dock's first group: its views in tab order and its active one.
fn right_dock(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> (Vec<ViewId>, Option<ViewId>) {
    window
        .update(cx, |shell, _, _| {
            let group = shell.group(shell.layout.dock_groups(Side::Right)[0]);
            let views = group.views().map(|(_, view)| view).collect();
            (views, group.shown.view())
        })
        .unwrap()
}

/// The pane is in no tab, the shell's content holds focus, and the layout holds.
fn assert_removed(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, window, cx| {
            assert_eq!(shell.tab_of(PANE), None);
            assert!(shell.focus_handle(cx).is_focused(window));
            shell.check_layout(cx);
        })
        .unwrap();
    assert!(cx.update(|_, cx| fakes::pane_input(cx)).is_none());
}

fn group_of(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> GroupId {
    window
        .update(cx, |shell, _, _| {
            shell.tab_of(PANE).expect("a tab holds the pane").0
        })
        .unwrap()
}

fn has_group(window: WindowHandle<AppShell>, cx: &mut VisualTestContext, group: GroupId) -> bool {
    window
        .update(cx, |shell, _, _| {
            shell.layout.groups().any(|held| held == group)
        })
        .unwrap()
}

#[gpui::test]
fn pane_feature_stays_dormant_until_its_tab_shows(cx: &mut TestAppContext) {
    let (_fs, window, mut cx) = pane_shell(cx);

    assert_eq!(fakes::pane_activations(), 0);
    let (views, active) = right_dock(window, &mut cx);
    assert_eq!(views.last(), Some(&PANE));
    assert_ne!(active, Some(PANE));

    click(&mut cx, "dock-icon-pane_fake");

    assert_eq!(fakes::pane_activations(), 1);
    assert_eq!(right_dock(window, &mut cx).1, Some(PANE));
}

#[gpui::test]
fn fake_pane_input_loses_focus_when_its_dock_hides(cx: &mut TestAppContext) {
    let (_fs, window, mut cx) = pane_shell(cx);
    click(&mut cx, "dock-icon-pane_fake");
    focus_input(window, &mut cx);

    cx.simulate_keystrokes("cmd-|");
    cx.run_until_parked();

    window
        .update(&mut cx, |shell, window, cx| {
            let (input, hidden) = fakes::pane_input(cx).expect("a hidden pane stays alive");
            assert_eq!(hidden, 1);
            assert!(!input.is_focused(window));
            assert!(shell.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn disabling_a_pane_feature_removes_its_tab_and_group(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = pane_shell(cx);
    cx.dispatch_action(SplitView {
        view: PANE,
        direction: SplitDirection::Down,
    });
    cx.run_until_parked();
    let group = group_of(window, &mut cx);
    focus_input(window, &mut cx);

    switch(&fs, false, &mut cx);

    assert_removed(window, &mut cx);
    assert!(!has_group(window, &mut cx, group));
}

#[gpui::test]
fn reenabled_pane_returns_last_and_inactive(cx: &mut TestAppContext) {
    let (fs, window, mut cx) = pane_shell(cx);
    click(&mut cx, "dock-icon-pane_fake");
    switch(&fs, false, &mut cx);

    switch(&fs, true, &mut cx);

    let (views, active) = right_dock(window, &mut cx);
    assert_eq!(views.last(), Some(&PANE));
    assert_ne!(active, Some(PANE));
    assert_eq!(fakes::pane_activations(), 1);
    assert!(cx.update(|_, cx| fakes::pane_input(cx)).is_none());
}

#[gpui::test]
fn disabling_a_pane_feature_shown_in_the_centre_closes_it_without_redocking(
    cx: &mut TestAppContext,
) {
    let (fs, window, mut cx) = pane_shell(cx);
    let notes = window
        .update(&mut cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    let a = test_support::entry_id(&notes, &mut cx, "a");
    open(&window, &mut cx, a);
    let closed = window
        .update(&mut cx, |shell, window, cx| {
            shell.move_view(PANE, shell.layout.last_center(), 1, window, cx);
            shell.center().tabs.has_closed()
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(fakes::pane_activations(), 1);

    switch(&fs, false, &mut cx);

    assert_removed(window, &mut cx);
    window
        .update(&mut cx, |shell, _, _| {
            assert_eq!(shell.showing(), Some(a));
            assert_eq!(shell.center().tabs.has_closed(), closed);
        })
        .unwrap();

    // Alone in a second centre group, the pane takes the group with it.
    switch(&fs, true, &mut cx);
    window
        .update(&mut cx, |shell, window, cx| {
            let tab = shell.tab_of(PANE).expect("back in its dock");
            shell.split_off(
                tab,
                shell.layout.last_center(),
                SplitDirection::Right,
                window,
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
    let group = group_of(window, &mut cx);
    assert_eq!(fakes::pane_activations(), 2);

    switch(&fs, false, &mut cx);

    assert!(!has_group(window, &mut cx, group));
    window
        .update(&mut cx, |shell, _, cx| {
            assert_eq!(shell.layout.center_groups().len(), 1);
            assert_eq!(shell.tab_of(PANE), None);
            shell.check_layout(cx);
        })
        .unwrap();
}
