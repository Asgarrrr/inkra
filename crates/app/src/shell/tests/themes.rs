use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::{no_vault, open_vault, shell_focused};
use crate::picker::PickerDelegate as _;
use crate::shell::AppShell;
use crate::theme::picker::ThemeDelegate;
use crate::theme::{
    self, Appearance, Theme, builtin,
    store::{list, load},
};

const SETTINGS: &str = "/vault/.inkra/settings.json";
const SOLAR: &str = "/vault/.inkra/themes/solar.json";

#[gpui::test]
fn a_missing_theme_shows_a_notice(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file(SETTINGS, r#"{"theme":"Gone"}"#);
    let (window, _sidebar, _notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let resolved = block_on(load(&*fs, Path::new("/vault")));
    let notice = window
        .update(cx, |shell, _, cx| {
            shell.adopt_theme(resolved, cx);
            shell.notice.as_ref().map(|notice| notice.text.to_string())
        })
        .unwrap();
    assert_eq!(notice.as_deref(), Some(r#"theme "Gone" not found"#));
}

fn solar(accent: &str) -> String {
    format!(r#"{{"appearance":"dark","colors":{{"accent":"{accent}"}}}}"#)
}

/// A vault launched on `solar`, as `bootstrap` adopts it.
fn open_solar(cx: &mut TestAppContext) -> (Arc<MemFs>, WindowHandle<AppShell>) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file(SETTINGS, r#"{"theme":"solar"}"#);
    fs.insert_file(SOLAR, solar("#111111"));
    let (window, _sidebar, _notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let resolved = block_on(load(&*fs, Path::new("/vault")));
    window
        .update(cx, |shell, _, cx| shell.adopt_theme(resolved, cx))
        .unwrap();
    cx.run_until_parked();
    (fs, window)
}

fn current(cx: &mut TestAppContext) -> Arc<Theme> {
    cx.update(|cx| theme::current(cx).clone())
}

#[gpui::test]
fn an_edit_to_the_active_theme_reloads_it(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);
    let before = current(cx);

    fs.insert_file(SOLAR, solar("#222222"));
    cx.run_until_parked();

    let edited = block_on(load(&*fs, Path::new("/vault")))
        .theme
        .expect("solar");
    assert_ne!(*edited, *before);
    assert_eq!(*current(cx), *edited);
}

#[gpui::test]
fn an_identical_echo_keeps_the_theme(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);
    let before = current(cx);
    let loads = fs.loads(Path::new(SOLAR));

    fs.insert_file(SOLAR, solar("#111111"));
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new(SOLAR)), loads + 1, "the echo reloads");
    assert!(Arc::ptr_eq(&current(cx), &before));
}

#[gpui::test]
fn an_edit_to_another_theme_loads_nothing(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);
    let loads = || (fs.loads(Path::new(SOLAR)), fs.loads(Path::new(SETTINGS)));
    let before = loads();

    fs.insert_file("/vault/.inkra/themes/other.json", solar("#333333"));
    cx.run_until_parked();

    assert_eq!(loads(), before);
}

#[gpui::test]
fn a_settings_edit_switches_the_theme(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);

    fs.insert_file(SETTINGS, r#"{"theme":"Flexoki Dark"}"#);
    cx.run_until_parked();

    assert!(Arc::ptr_eq(&current(cx), builtin::of(Appearance::Dark)));
    assert!(!cx.update(|cx| theme::follows_system(cx)));
}

#[gpui::test]
fn a_config_change_scans_nothing(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);
    let listed = fs.read_dirs(Path::new("/vault"));

    fs.insert_file(SETTINGS, r#"{"theme":"Flexoki Dark"}"#);
    cx.run_until_parked();

    assert_eq!(fs.read_dirs(Path::new("/vault")), listed);
}

#[gpui::test]
fn a_lost_events_rescan_reloads_the_theme(cx: &mut TestAppContext) {
    let (fs, _window) = open_solar(cx);
    let loads = fs.loads(Path::new(SETTINGS));

    fs.send_rescan("/vault");
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new(SETTINGS)), loads + 1);
}

/// The open theme picker's row labels and selected row; `None` when it is not open.
fn theme_picker(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Option<(Vec<String>, Option<usize>)> {
    window
        .update(cx, |shell, _, cx| {
            let picker = shell.modal_picker::<ThemeDelegate>()?.read(cx);
            let labels = (0..picker.delegate.match_count())
                .map(|ix| picker.delegate.row_label(ix).to_string())
                .collect();
            Some((labels, picker.selected()))
        })
        .expect("the shell is open")
}

/// From the palette, as a person picks it.
fn open_theme_picker(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-shift-p");
    cx.simulate_input("theme");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn down_previews_and_escape_restores(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (window, _notes, mut visual) = shell_focused(cx, &fs);
    visual.update(|_, cx| theme::install(None, cx));
    let light = builtin::of(Appearance::Light);

    open_theme_picker(&mut visual);
    let (_, selected) = theme_picker(window, &mut visual).expect("the theme picker is open");
    assert_eq!(selected, Some(0), "System");
    assert!(Arc::ptr_eq(&current(&mut visual), light));

    visual.simulate_keystrokes("down");
    visual.run_until_parked();
    assert!(Arc::ptr_eq(
        &current(&mut visual),
        builtin::of(Appearance::Dark)
    ));

    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(theme_picker(window, &mut visual).is_none());
    assert!(Arc::ptr_eq(&current(&mut visual), light));
    assert!(visual.update(|_, cx| theme::follows_system(cx)));
}

#[gpui::test]
fn without_a_vault_the_rows_are_the_builtins(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx);

    open_theme_picker(&mut visual);

    let (labels, _) = theme_picker(window, &mut visual).expect("the theme picker is open");
    assert_eq!(labels, ["System", "Flexoki Dark", "Flexoki Light"]);
}

/// A vault holding `solar.json` (accent #123456) and `settings`, keys bound, the root focused.
fn solar_vault(
    cx: &mut TestAppContext,
    settings: &str,
) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file(SOLAR, solar("#123456"));
    fs.insert_file(SETTINGS, settings);
    let (window, _notes, visual) = shell_focused(cx, &fs);
    (fs, window, visual)
}

fn solar_theme(fs: &MemFs) -> Arc<Theme> {
    let listed = block_on(list(fs, Path::new("/vault")));
    let (_, solar) = listed
        .into_iter()
        .find(|(name, _)| name == "solar")
        .expect("solar");
    solar
}

fn pick_solar(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    open_theme_picker(cx);
    let (labels, _) = theme_picker(window, cx).expect("the theme picker is open");
    assert!(labels.iter().any(|label| label == "solar"), "{labels:?}");
    cx.simulate_input("solar");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn enter_writes_the_choice_and_keeps_other_keys(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = solar_vault(cx, r#"{"font":3}"#);

    pick_solar(window, &mut visual);

    let written = fs.read_to_string(Path::new(SETTINGS)).expect("settings");
    let settings: serde_json::Value = serde_json::from_str(&written).expect("JSON");
    assert_eq!(settings, serde_json::json!({"theme": "solar", "font": 3}));
    assert!(theme_picker(window, &mut visual).is_none());
    assert_eq!(*current(&mut visual), *solar_theme(&fs));
}

#[gpui::test]
fn an_unwritable_settings_file_keeps_the_theme(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = solar_vault(cx, "{}");
    fs.make_unwritable(SETTINGS);

    pick_solar(window, &mut visual);

    let notice = window
        .update(&mut visual, |shell, _, _| {
            shell.notice.as_ref().map(|notice| notice.text.to_string())
        })
        .expect("the shell is open")
        .expect("a notice");
    assert!(
        notice.starts_with("Couldn't save the theme choice:"),
        "{notice}"
    );
    assert_eq!(*current(&mut visual), *solar_theme(&fs));
}

#[gpui::test]
fn a_reload_under_the_open_picker_is_what_escape_restores(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file(SETTINGS, r#"{"theme":"solar"}"#);
    fs.insert_file(SOLAR, solar("#111111"));
    let (window, _notes, mut visual) = shell_focused(cx, &fs);
    let resolved = block_on(load(&*fs, Path::new("/vault")));
    window
        .update(&mut visual, |shell, _, cx| shell.adopt_theme(resolved, cx))
        .unwrap();
    open_theme_picker(&mut visual);
    visual.simulate_keystrokes("up");
    visual.run_until_parked();

    fs.insert_file(SOLAR, solar("#222222"));
    visual.run_until_parked();
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();

    let edited = block_on(load(&*fs, Path::new("/vault")))
        .theme
        .expect("solar");
    assert_eq!(*current(&mut visual), *edited);
}

#[gpui::test]
fn a_theme_file_named_system_is_not_a_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/.inkra/themes/system.json", solar("#123456"));
    fs.insert_file(SETTINGS, "{}");
    let (window, _notes, mut visual) = shell_focused(cx, &fs);

    open_theme_picker(&mut visual);

    let (labels, _) = theme_picker(window, &mut visual).expect("the theme picker is open");
    assert_eq!(labels, ["System", "Flexoki Dark", "Flexoki Light"]);
}
