//! `⌘O` opens a chosen folder in place of the shell, after the shell left is written (open-a-vault spec, D3).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    EntityId, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowHandle, px, size,
};
use inkra_vault::mem_fs::MemFs;

use super::notices::notice;
use super::quitting::quitting;
use super::session_writes::{SESSION, saved_tabs, saving, tab, wait};
use super::{detached, no_vault, open, showing, vault_names};
use crate::app_files::session_file;
use crate::bootstrap::{Home, seed_session_in};
use crate::shell::AppShell;
use crate::shell::leave::{Leave, QUIT_WAIT};
use crate::test_support;
use crate::theme::{self, Appearance, builtin};

fn root_id(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> EntityId {
    window.entity(cx).unwrap().entity_id()
}

/// Answers the pending folder prompt with `path` and lets the open run.
fn choose(cx: &mut VisualTestContext, path: &str) {
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from(path)]));
    cx.run_until_parked();
}

fn cancel(cx: &mut VisualTestContext) {
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
}

fn cmd_o(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-o");
    cx.run_until_parked();
}

/// A later `⌘O` opens a panel: the leave was cleared.
fn prompts_again(cx: &mut VisualTestContext) -> bool {
    cmd_o(cx);
    cx.did_prompt_for_paths()
}

fn vault_root(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    window
        .update(cx, |shell, _, cx| {
            shell.vault_root(cx).map(|(_, root)| root)
        })
        .unwrap()
}

fn on_disk(fs: &MemFs, path: &str) -> Option<String> {
    fs.read_to_string(Path::new(path))
}

fn unsaved(title: &str) -> (String, String) {
    (
        "Some changes are not saved.".into(),
        format!("Not on disk: “{title}”."),
    )
}

#[gpui::test]
fn cmd_o_prompts_for_one_folder(cx: &mut TestAppContext) {
    let (_window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));

    cmd_o(&mut visual);
    assert!(visual.did_prompt_for_paths());
    let mut asked = None;
    visual.simulate_path_prompt_response(|options| {
        asked = Some((
            options.directories,
            options.files,
            options.multiple,
            options.prompt.clone(),
        ));
        None
    });

    assert_eq!(asked, Some((true, false, false, Some("Open Vault".into()))));
}

#[gpui::test]
fn cmd_o_opens_the_chosen_folder(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (window, mut visual) = no_vault(cx, &fs);
    let before = root_id(&window, &mut visual);

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");

    assert_ne!(root_id(&window, &mut visual), before);
    assert_eq!(
        vault_root(&window, &mut visual).as_deref(),
        Some(Path::new("/vault"))
    );
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(vault_names(&notes, &mut visual), ["a.md", "b.md"]);
    let focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .unwrap();
    assert!(focused, "the new shell takes the focus");
}

#[gpui::test]
fn the_open_button_opens_a_vault(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (window, mut visual) = no_vault(cx, &fs);

    let button = visual.debug_bounds("open-vault").unwrap();
    visual.simulate_click(button.center(), Modifiers::none());
    choose(&mut visual, "/vault");

    assert_eq!(
        vault_root(&window, &mut visual).as_deref(),
        Some(Path::new("/vault"))
    );
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(vault_names(&notes, &mut visual), ["a.md", "b.md"]);
}

#[gpui::test]
fn a_second_cmd_o_opens_no_second_prompt(cx: &mut TestAppContext) {
    let (_window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));

    cmd_o(&mut visual);
    cmd_o(&mut visual);
    cancel(&mut visual);

    assert!(!visual.did_prompt_for_paths());
}

#[gpui::test]
fn a_cancelled_prompt_changes_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));
    let before = root_id(&window, &mut visual);

    cmd_o(&mut visual);
    cancel(&mut visual);

    assert_eq!(root_id(&window, &mut visual), before);
    assert!(prompts_again(&mut visual));
}

#[gpui::test]
fn a_missing_folder_shows_a_notice(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));
    let before = root_id(&window, &mut visual);

    cmd_o(&mut visual);
    choose(&mut visual, "/nowhere");

    assert_eq!(
        notice(&window, &mut visual),
        Some(format!(
            "Couldn't open “nowhere”: {}",
            io::Error::from(io::ErrorKind::NotFound)
        ))
    );
    assert_eq!(root_id(&window, &mut visual), before);
    assert!(prompts_again(&mut visual));
}

#[gpui::test]
fn opening_the_open_vault_does_nothing(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    open(&window, &mut visual, a);
    let before = (root_id(&window, &mut visual), on_disk(&fs, SESSION));

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");

    assert_eq!(
        (root_id(&window, &mut visual), on_disk(&fs, SESSION)),
        before
    );
    assert!(prompts_again(&mut visual));
}

#[gpui::test]
fn a_detached_note_is_written_before_the_vault_opens(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (window, mut visual) = detached(cx, &fs);
    let before = root_id(&window, &mut visual);

    visual.simulate_input("x");
    cmd_o(&mut visual);
    choose(&mut visual, "/vault");

    assert_eq!(on_disk(&fs, "/notes/a.md").as_deref(), Some("xalpha"));
    assert_ne!(root_id(&window, &mut visual), before);
}

#[gpui::test]
fn an_unwritable_detached_note_asks_first(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.make_unwritable("/notes/a.md");
    let (window, mut visual) = detached(cx, &fs);
    let before = root_id(&window, &mut visual);
    visual.simulate_input("x");

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");
    assert_eq!(visual.pending_prompt(), Some(unsaved("a.md")));
    visual.simulate_prompt_answer("Cancel");
    visual.run_until_parked();
    assert_eq!(root_id(&window, &mut visual), before);

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");
    visual.simulate_prompt_answer("Open Anyway");
    visual.run_until_parked();
    assert_ne!(root_id(&window, &mut visual), before);
}

#[gpui::test]
fn switching_writes_the_left_vault_first(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");
    open(&window, &mut visual, a);
    visual.simulate_input("x");

    cmd_o(&mut visual);
    choose(&mut visual, "/other");

    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
    assert_eq!(saved_tabs(&fs), Some(vec![tab("a.md", 6, 6)]));
    assert_eq!(
        vault_root(&window, &mut visual).as_deref(),
        Some(Path::new("/other"))
    );
}

#[gpui::test]
fn a_quit_takes_over_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");
    open(&window, &mut visual, a);
    let before = root_id(&window, &mut visual);
    let gate = fs.block_save("/vault/a.md");
    visual.simulate_input("x");

    cmd_o(&mut visual);
    choose(&mut visual, "/other");
    visual.simulate_keystrokes("cmd-q");
    gate.release();
    visual.run_until_parked();

    assert_eq!(root_id(&window, &mut visual), before);
    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
    assert_eq!(saved_tabs(&fs), Some(vec![tab("a.md", 6, 6)]));
    let leaving = window
        .update(&mut visual, |shell, _, _| shell.leaving)
        .unwrap();
    assert_eq!(leaving, Some(Leave::Quit));
    assert!(!visual.has_pending_prompt());
}

/// Both waits run out on the hung save: only the quit, the leave under way, asks.
#[gpui::test]
fn a_quit_over_a_pending_open_asks_once(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");
    open(&window, &mut visual, a);
    let _gate = fs.block_save("/vault/a.md");
    visual.simulate_input("x");

    cmd_o(&mut visual);
    choose(&mut visual, "/other");
    visual.simulate_keystrokes("cmd-q");
    wait(&mut visual, QUIT_WAIT);

    assert_eq!(visual.pending_prompt(), Some(unsaved("a")));
    visual.simulate_prompt_answer("Quit Anyway");
    visual.run_until_parked();
    assert!(!visual.has_pending_prompt());
}

#[gpui::test]
fn the_new_vault_restores_its_tabs(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    seed_session_in(&*fs, Path::new("/vault"), "a.md", Path::new("/home")).unwrap();
    let file = session_file(Path::new("/home"), Path::new("/vault")).unwrap();
    let seeded = fs.read_to_string(&file);
    let (window, mut visual) = no_vault(cx, &fs);

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");

    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    let a = test_support::entry_id(&notes, &mut visual, "a");
    assert_eq!(showing(&window, &mut visual), Some(a));
    assert_eq!(fs.read_to_string(&file), seeded);
}

#[gpui::test]
fn the_new_vault_adopts_its_theme(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/.inkra/settings.json", r#"{"theme":"Flexoki Dark"}"#);
    let (_window, mut visual) = no_vault(cx, &fs);

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");

    let installed = visual.update(|_, cx| theme::current(cx).clone());
    assert!(Arc::ptr_eq(&installed, builtin::of(Appearance::Dark)));
}

#[gpui::test]
fn a_hung_read_stops_the_open_after_the_wait(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (window, mut visual) = no_vault(cx, &fs);
    let before = root_id(&window, &mut visual);
    let _gate = fs.block_load(session_file(Path::new("/home"), Path::new("/vault")).unwrap());

    cmd_o(&mut visual);
    choose(&mut visual, "/vault");
    wait(&mut visual, QUIT_WAIT);

    assert_eq!(
        notice(&window, &mut visual),
        Some(format!(
            "Couldn't open “vault”: {}",
            io::Error::from(io::ErrorKind::TimedOut)
        ))
    );
    assert_eq!(root_id(&window, &mut visual), before);
    assert!(prompts_again(&mut visual));
}

#[gpui::test]
fn the_old_vault_is_released(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let (fs, window, mut visual, _a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");
    let (notes, vault) = window
        .update(&mut visual, |shell, _, cx| {
            let vault = shell.notes.read(cx).vault().expect("a vault").downgrade();
            (shell.notes.downgrade(), vault)
        })
        .unwrap();

    cmd_o(&mut visual);
    choose(&mut visual, "/other");
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();

    assert!(notes.upgrade().is_none(), "the old store is dropped");
    assert!(vault.upgrade().is_none(), "the old vault is dropped");
}

#[gpui::test]
fn the_old_shell_hears_nothing(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let (fs, window, mut visual, _a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");
    let old = window.entity(&visual).unwrap().downgrade();

    cmd_o(&mut visual);
    choose(&mut visual, "/other");
    visual.simulate_resize(size(px(800.), px(600.)));
    visual.run_until_parked();

    assert!(old.upgrade().is_none());
}

#[gpui::test]
fn closing_after_a_switch_runs_the_new_quit(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Home("/home".into())));
    let (fs, window, mut visual, _a, _b) = saving(cx);
    fs.insert_file("/other/c.md", "gamma");

    cmd_o(&mut visual);
    choose(&mut visual, "/other");

    assert!(
        !visual.simulate_close(),
        "the new shell's quit closes the window"
    );
    assert!(quitting(&window, &mut visual));
}
