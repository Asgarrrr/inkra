use std::path::Path;

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;

use super::{Settings, Switches, read};
use crate::theme::store::Choice;

fn settings(text: &str) -> (Settings, Vec<String>) {
    let fs = MemFs::new();
    fs.insert_file("/v/.inkra/settings.json", text);
    let mut notices = Vec::new();
    let settings = block_on(read(&fs, Path::new("/v/.inkra"), &mut notices));
    (settings, notices)
}

#[test]
fn code_block_wrap_reads_true_false_missing_and_non_bool() {
    for (text, expected) in [
        (r#"{"code_block_wrap":true}"#, true),
        (r#"{"code_block_wrap":false}"#, false),
        ("{}", false),
        (r#"{"code_block_wrap":"yes"}"#, false),
    ] {
        let (settings, notices) = settings(text);
        assert_eq!(settings.code_block_wrap, expected, "{text}");
        assert!(notices.is_empty(), "{text}");
    }
}

#[test]
fn a_missing_file_reads_the_default_switches() {
    let mut notices = Vec::new();
    let settings = block_on(read(&MemFs::new(), Path::new("/v/.inkra"), &mut notices));
    assert_eq!(settings.features, Some(Switches::default()));
    assert!(notices.is_empty());
}

#[test]
fn an_unreadable_file_reads_no_switches_and_says_why() {
    let fs = MemFs::new();
    fs.insert_file("/v/.inkra/settings.json", "{}");
    fs.make_unreadable("/v/.inkra/settings.json");
    let mut notices = Vec::new();
    let settings = block_on(read(&fs, Path::new("/v/.inkra"), &mut notices));
    assert_eq!(settings.features, None);
    assert_eq!(notices.len(), 1, "{notices:?}");
}

#[test]
fn broken_settings_fall_back_with_their_line() {
    let (settings, notices) = settings("{");
    assert_eq!(settings.choice, Choice::System);
    assert_eq!(settings.features, None);
    let [notice] = notices.as_slice() else {
        panic!("one notice: {notices:?}");
    };
    assert!(
        notice.starts_with(".inkra/settings.json: line 1: "),
        "{notice}"
    );
}

#[test]
fn features_keeps_bools_and_ignores_the_rest() {
    let (settings, notices) =
        settings(r#"{"features":{"on":true,"off":false,"word":"yes","number":1,"null":null}}"#);
    assert!(notices.is_empty());
    let switches = settings.features.expect("readable");
    assert_eq!(switches.get("on"), Some(true));
    assert_eq!(switches.get("off"), Some(false));
    for ignored in ["word", "number", "null", "absent"] {
        assert_eq!(switches.get(ignored), None, "{ignored}");
    }
}

#[test]
fn features_not_an_object_is_empty() {
    for text in [
        "{}",
        r#"{"features":true}"#,
        r#"{"features":["on"]}"#,
        r#"{"features":"on"}"#,
        r#"{"features":null}"#,
    ] {
        let (settings, notices) = settings(text);
        assert_eq!(settings.features, Some(Switches::default()), "{text}");
        assert!(notices.is_empty(), "{text}");
    }
}
