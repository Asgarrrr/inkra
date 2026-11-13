//! Features in a real shell: the switch, the gated key, the palette entry, the release (feature registry spec).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use gpui::{App, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::open_vault_with;
use super::themes::pick;
use crate::feature::{self, Registry, fakes};
use crate::keymap::key_bindings;
use crate::settings::{self, Switches};
use crate::shell::features::over_budget;
use crate::shell::{AppShell, Left, NoteStore};

const SETTINGS: &str = "/vault/.inkra/settings.json";

fn bind(cx: &mut App) {
    cx.bind_keys(key_bindings());
    cx.bind_keys(feature::bindings(fakes::CATALOG));
}

fn fakes_on() -> Registry {
    Registry::new(fakes::CATALOG, &Switches::default())
}

/// `/vault/a.md` behind a shell with the fake catalog, its root focused.
fn ping_shell(cx: &mut TestAppContext) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    cx.update(bind);
    let (window, _sidebar, _notes) =
        open_vault_with(cx, Arc::clone(&fs) as Arc<dyn Fs>, fakes_on());
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    focus_root(window, &mut visual);
    (fs, window, visual)
}

fn focus_root(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle, cx);
        })
        .unwrap();
    cx.run_until_parked();
}

fn switch_ping(fs: &MemFs, on: bool, cx: &mut VisualTestContext) {
    fs.insert_file(SETTINGS, format!(r#"{{"features":{{"ping_fake":{on}}}}}"#));
    cx.run_until_parked();
}

fn ping(cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|_, cx| fakes::ping_count(cx))
}

fn press_ping(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("ctrl-alt-p");
    cx.run_until_parked();
}

fn names_ping(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.key_context().contains("ping_fake"))
        .unwrap()
}

/// `⌘O` to `/b`, the shell rebuilt on it.
fn open_b(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-o");
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/b")]));
    cx.run_until_parked();
    let root = window
        .update(cx, |shell, _, cx| {
            shell.vault_root(cx).map(|(_, root)| root)
        })
        .unwrap();
    assert_eq!(root, Some(PathBuf::from("/b")));
}

fn palette_lists_ping(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    let names = window
        .update(cx, |shell, _, cx| shell.palette_names(cx))
        .unwrap()
        .expect("the palette is open");
    cx.simulate_keystrokes("cmd-shift-p");
    focus_root(window, cx);
    names.iter().any(|label| {
        label
            .split_once(", ")
            .map_or(label.as_str(), |(name, _)| name)
            == "Ping fake"
    })
}

#[gpui::test]
fn ping_on_off_on_by_keystroke(cx: &mut TestAppContext) {
    let (fs, _window, mut visual) = ping_shell(cx);
    assert_eq!(ping(&mut visual), None, "dormant until its first command");

    press_ping(&mut visual);
    press_ping(&mut visual);
    assert_eq!(ping(&mut visual), Some(2));

    switch_ping(&fs, false, &mut visual);
    press_ping(&mut visual);
    switch_ping(&fs, true, &mut visual);
    press_ping(&mut visual);
    assert_eq!(ping(&mut visual), Some(1), "the key did nothing while off");
}

#[gpui::test]
fn ping_palette_entry_follows_the_switch(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = ping_shell(cx);
    assert!(palette_lists_ping(window, &mut visual));

    switch_ping(&fs, false, &mut visual);
    assert!(!palette_lists_ping(window, &mut visual));

    switch_ping(&fs, true, &mut visual);
    assert!(palette_lists_ping(window, &mut visual));
}

#[gpui::test]
fn disabled_ping_entity_is_released(cx: &mut TestAppContext) {
    let (fs, _window, mut visual) = ping_shell(cx);
    press_ping(&mut visual);
    assert_eq!(ping(&mut visual), Some(1));

    switch_ping(&fs, false, &mut visual);
    assert_eq!(ping(&mut visual), None, "off drops the handle that held it");
}

#[gpui::test]
fn key_context_names_enabled_features_only(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = ping_shell(cx);
    let context = window
        .update(&mut visual, |shell, _, _| shell.key_context())
        .unwrap();
    assert!(context.contains("AppShell"));
    assert!(context.contains("ping_fake"), "dormant is enabled");

    switch_ping(&fs, false, &mut visual);
    let context = window
        .update(&mut visual, |shell, _, _| shell.key_context())
        .unwrap();
    assert!(context.contains("AppShell"));
    assert!(!context.contains("ping_fake"));
}

#[gpui::test]
fn no_vault_renders_no_feature(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new()) as Arc<dyn Fs>;
    cx.update(bind);
    let notes = cx.update(|cx| gpui::AppContext::new(cx, |cx| NoteStore::new(fs, None, cx)));
    let window = cx.add_window(move |window, cx| {
        AppShell::new(
            None,
            notes,
            None,
            None,
            Left::default(),
            fakes_on(),
            window,
            cx,
        )
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    focus_root(window, &mut visual);

    assert!(!names_ping(window, &mut visual));
    assert!(!palette_lists_ping(window, &mut visual));
    press_ping(&mut visual);
    assert_eq!(ping(&mut visual), None);
}

#[gpui::test]
fn open_vault_b_never_builds_a_disabled_feature(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = ping_shell(cx);
    fs.insert_file("/b/x.md", "x");
    fs.insert_file(
        "/b/.inkra/settings.json",
        r#"{"features":{"ping_fake":false}}"#,
    );
    assert!(names_ping(window, &mut visual));

    open_b(window, &mut visual);

    press_ping(&mut visual);
    assert_eq!(ping(&mut visual), None);
    assert!(!names_ping(window, &mut visual));
}

/// `/vault/a.md` behind a shell with the eager catalog, switched by `settings`; nothing has run since the window opened.
fn eager_shell(
    cx: &mut TestAppContext,
    settings: Option<&str>,
) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    if let Some(settings) = settings {
        fs.insert_file(SETTINGS, settings);
    }
    let switches = block_on(settings::read(
        &*fs,
        Path::new("/vault/.inkra"),
        &mut Vec::new(),
    ))
    .features
    .unwrap_or_default();
    cx.update(bind);
    let (window, _sidebar, _notes) = open_vault_with(
        cx,
        Arc::clone(&fs) as Arc<dyn Fs>,
        Registry::new(fakes::EAGER_CATALOG, &switches),
    );
    let visual = VisualTestContext::from_window(window.into(), cx);
    (fs, window, visual)
}

#[gpui::test]
fn eager_feature_activates_after_the_first_draw(cx: &mut TestAppContext) {
    let (_fs, _window, visual) = eager_shell(cx, None);
    assert_eq!(
        fakes::eager_count(),
        0,
        "not inside the shell's build or first draw"
    );

    visual.run_until_parked();
    assert_eq!(fakes::eager_count(), 1);
}

#[gpui::test]
fn open_vault_b_keeps_a_disabled_eager_feature_off(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = eager_shell(cx, None);
    focus_root(window, &mut visual);
    assert_eq!(fakes::eager_count(), 1);
    fs.insert_file("/b/x.md", "x");
    fs.insert_file(
        "/b/.inkra/settings.json",
        r#"{"features":{"eager_fake":false}}"#,
    );

    open_b(window, &mut visual);
    assert_eq!(fakes::eager_count(), 1);
    let named = window
        .update(&mut visual, |shell, _, _| {
            shell.key_context().contains("eager_fake")
        })
        .unwrap();
    assert!(!named);
}

#[gpui::test]
fn eager_feature_switched_on_after_launch_activates_at_once(cx: &mut TestAppContext) {
    let (fs, _window, visual) = eager_shell(cx, Some(r#"{"features":{"eager_fake":false}}"#));
    visual.run_until_parked();
    assert_eq!(fakes::eager_count(), 0);

    fs.insert_file(SETTINGS, r#"{"features":{"eager_fake":true}}"#);
    visual.run_until_parked();
    assert_eq!(fakes::eager_count(), 1);
}

#[test]
fn over_budget_is_strictly_above_two_ms() {
    assert!(!over_budget(Duration::from_millis(2)));
    assert!(over_budget(
        Duration::from_millis(2) + Duration::from_micros(1)
    ));
}

#[gpui::test]
fn a_theme_pick_keeps_a_switch_reload_in_flight(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = ping_shell(cx);
    fs.make_unwritable(SETTINGS);
    let reload = fs.block_read_bytes(SETTINGS);
    fs.insert_file(SETTINGS, r#"{"features":{"ping_fake":false}}"#);
    visual.run_until_parked();

    pick("flexoki dark", window, &mut visual);
    reload.release();
    visual.run_until_parked();

    assert!(!names_ping(window, &mut visual));
}

#[gpui::test]
fn a_broken_settings_file_keeps_the_switches(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = ping_shell(cx);
    switch_ping(&fs, false, &mut visual);
    assert!(!names_ping(window, &mut visual));

    fs.insert_file(SETTINGS, "{");
    visual.run_until_parked();

    assert!(!names_ping(window, &mut visual));
}
