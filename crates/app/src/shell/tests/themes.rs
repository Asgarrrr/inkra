use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use gpui::{TestAppContext, WindowHandle};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::open_vault;
use crate::shell::AppShell;
use crate::theme::{self, Appearance, Theme, builtin, store::load};

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
