use std::path::Path;

use gpui::{Bounds, TestAppContext, point, px, size};
use inkra_vault::mem_fs::MemFs;

use super::{Launch, WINDOW_SIZE, open_argument, placement, vault_root};
use crate::restore::SavedWindow;

/// One vault, one session file: the key hashes these bytes, so compare bytes, not `Path`'s components.
#[test]
fn a_vault_root_drops_a_trailing_slash_and_dot_components() {
    for spelling in ["/Users/me/Notes/", "/Users/me/./Notes", "/Users/me/Notes"] {
        assert_eq!(
            vault_root(Path::new(spelling)).unwrap().as_os_str(),
            "/Users/me/Notes"
        );
    }
}

/// `Fs` tells a vault from a note, so the launch never stats the disk behind it.
#[test]
fn an_argument_opens_a_folder_as_a_vault_and_a_file_as_a_note() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    fs.insert_file("/vault/a.md", "alpha");
    assert!(matches!(
        open_argument(&fs, "/vault".into(), None),
        Ok(Launch::Vault(vault, None, None)) if vault.root() == Path::new("/vault")
    ));
    assert!(matches!(
        open_argument(&fs, "/vault/a.md".into(), None),
        Ok(Launch::Note(path, text, _)) if path == Path::new("/vault/a.md") && text == "alpha"
    ));
}

#[test]
fn a_missing_argument_or_an_unreadable_folder_is_an_error_naming_it() {
    let fs = MemFs::new();
    fs.insert_dir("/locked");
    fs.make_unreadable("/locked");
    assert!(
        open_argument(&fs, "/nowhere.md".into(), None)
            .is_err_and(|error| error.contains("/nowhere.md"))
    );
    assert!(
        open_argument(&fs, "/locked".into(), None).is_err_and(|error| error.contains("/locked"))
    );
}

#[gpui::test]
fn a_saved_window_opens_on_its_display_and_one_gone_or_off_its_display_centres(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let display = cx.primary_display().expect("the test display");
        let saved = SavedWindow {
            display: display.uuid().unwrap().to_string(),
            x: 120.,
            y: 80.,
            width: 1200.,
            height: 760.,
        };
        let frame = Bounds::new(point(px(120.), px(80.)), size(px(1200.), px(760.)));
        assert_eq!(placement(Some(&saved), cx), (Some(display.id()), frame));

        let gone = SavedWindow {
            display: "a display unplugged since".to_owned(),
            ..saved.clone()
        };
        let centred = (None, Bounds::centered(None, WINDOW_SIZE, cx));
        assert_eq!(placement(Some(&gone), cx), centred);
        let off = SavedWindow {
            x: 2000.,
            ..saved.clone()
        };
        assert_eq!(placement(Some(&off), cx), centred);
        assert_eq!(placement(None, cx), centred);
    });
}
