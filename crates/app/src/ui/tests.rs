#![allow(clippy::expect_used)]

use std::path::Path;

use gpui::AssetSource;

use super::{Assets, IconName};

#[test]
fn every_icon_loads() {
    for name in IconName::ALL {
        let bytes = Assets
            .load(name.path())
            .expect("load")
            .unwrap_or_else(|| panic!("{name:?} has no file"));
        assert!(bytes.starts_with(b"<svg"), "{name:?} is not an svg");
    }
}

#[test]
fn no_icon_file_is_dangling() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    for file in std::fs::read_dir(&dir).expect("the icons directory") {
        let file = file.expect("an entry").path();
        if file.extension().is_none_or(|extension| extension != "svg") {
            continue;
        }
        let name = file.file_name().expect("a file name").to_string_lossy();
        let path = format!("icons/{name}");
        assert!(
            IconName::ALL.iter().any(|icon| icon.path() == path),
            "{path} is no `IconName`"
        );
    }
}

#[test]
fn unknown_path_is_none() {
    assert!(Assets.load("icons/nope.svg").expect("load").is_none());
}

#[test]
fn every_icon_has_a_distinct_path() {
    for (ix, name) in IconName::ALL.iter().enumerate() {
        for other in &IconName::ALL[ix + 1..] {
            assert_ne!(name.path(), other.path(), "{name:?} and {other:?}");
        }
    }
}
