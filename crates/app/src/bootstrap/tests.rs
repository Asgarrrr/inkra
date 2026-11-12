use std::io;
use std::path::Path;
use std::sync::Arc;

use gpui::{Bounds, TestAppContext, point, px, size};
use inkra_vault::mem_fs::MemFs;

use super::{
    Launch, WINDOW_SIZE, open_argument, placement, remembered, seed_session_in, vault_launch,
    vault_root,
};
use crate::app_files::{cache_file, last_vault_file, session_file};
use crate::last_vault;
use crate::restore::{self, SavedFileTab, SavedGroup, SavedMember, SavedTab, SavedWindow};

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

/// An unreadable folder fails the launch itself; loading it as a note would hide why.
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
    assert_eq!(fs.loads(Path::new("/locked")), 0);
}

/// The seed is read back by the launch itself: its file name, root and format are the app's.
#[test]
fn a_seeded_session_is_the_one_the_launch_reads() {
    let fs = Arc::new(MemFs::new());
    fs.insert_dir("/vault");
    fs.insert_file("/vault/a.md", "alpha");
    seed_session_in(&*fs, Path::new("/vault/"), "a.md", Path::new("/home")).unwrap();

    let Ok(Launch::Vault(vault, Some(file), _)) =
        open_argument(&*fs, "/vault".into(), Some(Path::new("/home")))
    else {
        panic!("the folder opens as a vault with a session file");
    };
    let session = futures::executor::block_on(restore::read(fs, file, vault.root().to_path_buf()))
        .expect("the launch reads the seed");
    assert_eq!(
        session.center,
        Some(SavedMember::Group(SavedGroup {
            tabs: vec![SavedTab::File(SavedFileTab {
                file: Some("a.md".to_owned()),
                view: None,
            })],
            active: 0,
        }))
    );
    assert_eq!((session.left, session.right), (None, None));
}

/// A launch with no argument after `/vault` was opened, under the home `/home`.
fn relaunch(fs: &MemFs) -> Launch {
    let home = Path::new("/home");
    futures::executor::block_on(last_vault::write(fs, home, Path::new("/vault")));
    remembered(fs, home)
}

fn notice(launched: Launch) -> Option<String> {
    let Launch::Nothing { notice } = launched else {
        panic!("the launch opens nothing");
    };
    notice
}

#[test]
fn the_last_vault_opens_at_a_launch_with_no_argument() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    let (home, root) = (Path::new("/home"), Path::new("/vault"));

    assert!(matches!(
        relaunch(&fs),
        Launch::Vault(vault, session, cache)
            if vault.root() == root
                && session == session_file(home, root)
                && cache == cache_file(home, root)
    ));
}

#[test]
fn a_gone_last_vault_opens_nothing_and_says_so() {
    assert_eq!(
        notice(relaunch(&MemFs::new())).as_deref(),
        Some("The last vault, “vault”, is no longer there.")
    );
}

/// `open_argument` would load the file as a detached note (open-a-vault spec, D5).
#[test]
fn a_file_at_the_last_vault_opens_nothing_and_never_a_note() {
    let fs = MemFs::new();
    fs.insert_file("/vault", "alpha");

    assert_eq!(
        notice(relaunch(&fs)).as_deref(),
        Some("The last vault, “vault”, is no longer there.")
    );
}

#[test]
fn an_unreadable_last_vault_opens_nothing_and_says_why() {
    let fs = MemFs::new();
    fs.insert_dir("/vault");
    fs.make_unreadable("/vault");

    assert_eq!(
        notice(relaunch(&fs)),
        Some(format!(
            "Couldn't open “vault”: {}",
            io::Error::from(io::ErrorKind::PermissionDenied)
        ))
    );
}

#[test]
fn a_corrupt_last_vault_file_opens_nothing_silently() {
    let fs = MemFs::new();
    let home = Path::new("/home");
    fs.insert_file(last_vault_file(home), "garbage");

    assert_eq!(notice(remembered(&fs, home)), None);
}

const HOLDS_HOME: &str = "“me” holds Inkra's own files. Choose a folder inside it.";

/// `/home/me/Library/Caches` holds the cache alone: checking only the session accepts it.
#[test]
fn a_root_holding_the_app_files_is_refused() {
    let home = Some(Path::new("/home/me"));
    for root in [
        "/home/me",
        "/home/me/Library",
        "/home/me/Library/Caches",
        "/home",
        "/",
    ] {
        assert!(
            vault_launch(Path::new(root), home).is_err(),
            "{root} was accepted"
        );
    }
    assert_eq!(
        vault_launch(Path::new("/home/me"), home).err().as_deref(),
        Some(HOLDS_HOME)
    );
}

#[test]
fn a_root_inside_home_is_accepted() {
    assert!(matches!(
        vault_launch(Path::new("/home/me/Notes"), Some(Path::new("/home/me"))),
        Ok(Launch::Vault(_, Some(_), Some(_)))
    ));
}

#[test]
fn without_home_any_root_opens_without_session_or_cache() {
    assert!(matches!(
        vault_launch(Path::new("/home/me"), None),
        Ok(Launch::Vault(_, None, None))
    ));
}

#[test]
fn the_home_folder_as_the_argument_is_refused_by_name() {
    let fs = MemFs::new();
    fs.insert_dir("/home/me");

    assert!(
        open_argument(&fs, "/home/me".into(), Some(Path::new("/home/me")))
            .err()
            .is_some_and(|error| error.contains("“me”"))
    );
}

#[test]
fn a_last_vault_naming_the_home_folder_opens_nothing_and_says_so() {
    let fs = MemFs::new();
    let home = Path::new("/home/me");
    fs.insert_dir(home);
    futures::executor::block_on(last_vault::write(&fs, home, home));

    assert_eq!(notice(remembered(&fs, home)).as_deref(), Some(HOLDS_HOME));
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
            ..saved
        };
        let centred = (None, Bounds::centered(None, WINDOW_SIZE, cx));
        assert_eq!(placement(Some(&gone), cx), centred);
        let off = SavedWindow { x: 2000., ..saved };
        assert_eq!(placement(Some(&off), cx), centred);
        assert_eq!(placement(None, cx), centred);
    });
}
