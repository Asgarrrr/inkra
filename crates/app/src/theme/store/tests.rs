use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use inkra_vault::RelPath;
use inkra_vault::mem_fs::MemFs;
use serde_json::{Value, json};

use super::{Choice, MAX_CONFIG_BYTES, Resolved, ThemeStore, list, load, write_choice};
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
    assert_eq!(fs.reads(Path::new("/v/.inkra/x.json")), 0);
    assert_eq!(fs.reads(Path::new("/v/.inkra/themes/../x.json")), 0);
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
        ..ThemeStore::default()
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

const SETTINGS: &str = "/v/.inkra/settings.json";

fn settings(fs: &MemFs) -> Option<String> {
    fs.read_to_string(Path::new(SETTINGS))
}

fn write(fs: &MemFs, choice: &Choice) -> Result<(), String> {
    block_on(write_choice(fs, Path::new("/v"), choice))
}

#[test]
fn writing_the_choice_keeps_other_keys() {
    let solar = Choice::Named("solar".into());
    for (before, after) in [
        (None, json!({"theme": "solar"})),
        (Some(r#"{"font":3}"#), json!({"font": 3, "theme": "solar"})),
    ] {
        let fs = vault(before);
        assert_eq!(write(&fs, &solar), Ok(()), "{before:?}");
        let written = settings(&fs).expect("written");
        assert!(written.ends_with('\n'), "{written:?}");
        let value: Value = serde_json::from_str(&written).expect("JSON");
        assert_eq!(value, after, "{before:?}");
    }
}

#[test]
fn write_choice_keeps_code_block_wrap() {
    let fs = vault(Some(r#"{"code_block_wrap":true}"#));
    assert_eq!(write(&fs, &Choice::Named("solar".into())), Ok(()));
    let value: Value = serde_json::from_str(&settings(&fs).expect("written")).expect("JSON");
    assert_eq!(value, json!({"code_block_wrap": true, "theme": "solar"}));
}

#[test]
fn the_system_choice_is_written_as_system() {
    let fs = vault(Some(r#"{"theme":"solar"}"#));
    assert_eq!(write(&fs, &Choice::System), Ok(()));
    let value: Value = serde_json::from_str(&settings(&fs).expect("written")).expect("JSON");
    assert_eq!(value, json!({"theme": "system"}));
}

#[test]
fn settings_that_are_not_an_object_are_left_alone() {
    for before in ["[1]", "{"] {
        let fs = vault(Some(before));
        assert_eq!(
            write(&fs, &Choice::Named("solar".into())),
            Err(
                "Couldn't save the theme choice: .inkra/settings.json is not a JSON object"
                    .to_owned()
            ),
            "{before}"
        );
        assert_eq!(settings(&fs).as_deref(), Some(before));
    }
}

#[test]
fn listing_keeps_parsable_json_themes_by_stem_but_not_system_or_other_cases() {
    let fs = vault(None);
    for (file, text) in [
        ("solar.json", SOLAR),
        ("Amber.JSON", SOLAR),
        ("system.json", SOLAR),
        ("notes.txt", SOLAR),
        ("broken.json", "{"),
        ("Flexoki Dark.json", SOLAR),
    ] {
        fs.insert_file(format!("/v/.inkra/themes/{file}"), text);
    }
    let names: Vec<String> = block_on(list(&fs, Path::new("/v")))
        .into_iter()
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(names, ["solar"]);
    assert!(block_on(list(&vault(None), Path::new("/v"))).is_empty());
}

/// `json` with spaces before its closing brace, `over` bytes past the cap.
fn padded(json: &str, over: usize) -> String {
    let cap = usize::try_from(MAX_CONFIG_BYTES).expect("the cap fits in memory");
    let body = json.strip_suffix('}').expect("an object");
    let pad = (cap + over).saturating_sub(json.len());
    format!("{body}{}}}", " ".repeat(pad))
}

#[test]
fn settings_at_the_cap_load_and_one_byte_over_fall_back_with_a_notice() {
    let dark = r#"{"theme":"Flexoki Dark"}"#;

    let at = resolve(&vault(Some(&padded(dark, 0))));
    assert!(at.theme.is_some(), "a file of exactly the cap loads");
    assert!(at.notices.is_empty());

    let over = resolve(&vault(Some(&padded(dark, 1))));
    assert!(over.theme.is_none());
    assert_eq!(over.choice, Choice::System);
    assert_eq!(over.notices.len(), 1);
    assert!(
        over.notices[0].contains("settings.json"),
        "{:?}",
        over.notices
    );
}

#[test]
fn a_theme_file_over_the_cap_is_neither_applied_nor_listed() {
    let fs = vault(Some(r#"{"theme":"solar"}"#));
    fs.insert_file("/v/.inkra/themes/solar.json", padded(SOLAR, 1));

    let resolved = resolve(&fs);
    assert!(resolved.theme.is_none());
    assert_eq!(resolved.notices.len(), 1);
    assert!(
        resolved.notices[0].contains("solar.json"),
        "{:?}",
        resolved.notices
    );
    assert!(block_on(list(&fs, Path::new("/v"))).is_empty());
}

#[test]
fn a_choice_is_not_written_into_settings_over_the_cap() {
    let big = padded(r#"{"theme":"Flexoki Dark"}"#, 1);
    let fs = vault(Some(&big));

    let written = write(&fs, &Choice::System);

    assert!(written.is_err());
    assert_eq!(settings(&fs), Some(big));
}
