use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use inkra_vault::RelPath;
use inkra_vault::mem_fs::MemFs;

use super::{Choice, Resolved, ThemeStore, load};
use crate::theme::{Appearance, builtin};

const SOLAR: &str = r##"{"appearance":"light","colors":{"accent":"#ff0000"}}"##;

fn vault(settings: Option<&str>) -> MemFs {
    let fs = MemFs::new();
    fs.insert_file("/v/a.md", "a");
    if let Some(settings) = settings {
        fs.insert_file("/v/.inkra/settings.json", settings);
    }
    fs
}

fn resolve(fs: &MemFs) -> Resolved {
    block_on(load(fs, Path::new("/v")))
}

#[test]
fn no_choice_follows_the_system_silently() {
    for settings in [
        None,
        Some(r#"{"theme":"system"}"#),
        Some(r#"{"theme":3}"#),
        Some(r#"{"other":true}"#),
    ] {
        let resolved = resolve(&vault(settings));
        assert!(resolved.theme.is_none(), "{settings:?}");
        assert!(resolved.notices.is_empty(), "{settings:?}");
    }
}

#[test]
fn a_builtin_name_wins_over_a_file_of_that_name() {
    let fs = vault(Some(r#"{"theme":"Flexoki Light"}"#));
    fs.insert_file("/v/.inkra/themes/Flexoki Light.json", SOLAR);
    let resolved = resolve(&fs);
    let theme = resolved.theme.expect("the builtin");
    assert!(Arc::ptr_eq(&theme, builtin::of(Appearance::Light)));
    assert!(resolved.notices.is_empty());
}

#[test]
fn a_vault_theme_is_read_from_its_file() {
    let fs = vault(Some(r#"{"theme":"solar"}"#));
    fs.insert_file("/v/.inkra/themes/solar.json", SOLAR);
    let resolved = resolve(&fs);
    let theme = resolved.theme.expect("solar");
    assert_eq!(theme.appearance, Appearance::Light);
    assert_eq!(theme.colors.accent, gpui::rgba(0xff_00_00_ff));
    assert!(resolved.notices.is_empty());
}

#[test]
fn a_missing_theme_falls_back_with_a_notice() {
    let resolved = resolve(&vault(Some(r#"{"theme":"Gone"}"#)));
    assert!(resolved.theme.is_none());
    assert_eq!(resolved.notices, [r#"theme "Gone" not found"#]);
}

#[test]
fn a_name_is_never_a_path() {
    let fs = vault(Some(r#"{"theme":"../x"}"#));
    let resolved = resolve(&fs);
    assert!(resolved.theme.is_none());
    assert_eq!(resolved.notices, [r#"theme "../x" not found"#]);
    assert_eq!(fs.loads(Path::new("/v/.inkra/x.json")), 0);
    assert_eq!(fs.loads(Path::new("/v/.inkra/themes/../x.json")), 0);
}

#[test]
fn broken_settings_fall_back_with_their_line() {
    let resolved = resolve(&vault(Some("{")));
    assert!(resolved.theme.is_none());
    let [notice] = resolved.notices.as_slice() else {
        panic!("one notice: {:?}", resolved.notices);
    };
    assert!(
        notice.starts_with(".inkra/settings.json: line 1: "),
        "{notice}"
    );
}

#[test]
fn skipped_keys_are_reported_and_the_theme_still_applies() {
    let fs = vault(Some(r#"{"theme":"solar"}"#));
    fs.insert_file(
        "/v/.inkra/themes/solar.json",
        r##"{"appearance":"dark","colors":{"nope":"#fff","accent":"red","text":"#010203"}}"##,
    );
    let resolved = resolve(&fs);
    assert!(resolved.theme.is_some());
    assert_eq!(
        resolved.notices,
        [r#"theme "solar.json": skipped accent, nope"#]
    );
}

#[test]
fn a_store_concerns_the_settings_and_only_its_own_theme_file() {
    let store = |choice: Choice| ThemeStore {
        choice,
        reload: None,
    };
    let paths = [
        ".inkra",
        ".inkra/settings.json",
        ".inkra/themes",
        ".inkra/themes/solar.json",
        ".inkra/themes/other.json",
        ".inkra/settings.json.bak",
    ];
    for (choice, expected) in [
        (Choice::System, [true, true, false, false, false, false]),
        (
            Choice::Named("Flexoki Dark".into()),
            [true, true, false, false, false, false],
        ),
        (
            Choice::Named("solar".into()),
            [true, true, true, true, false, false],
        ),
    ] {
        let store = store(choice.clone());
        let concerns =
            paths.map(|path| store.concerns(&RelPath::new(Path::new(path)).expect("relative")));
        assert_eq!(concerns, expected, "{choice:?}");
    }
}
