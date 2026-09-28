use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use futures::channel::mpsc;

use super::{Changed, MAX_FOLDERS, PathEvent, Pending, Watch};
use crate::RelPath;
use crate::fs::Watcher;
use crate::temp::temp_path;

struct Unwatched;

impl Watcher for Unwatched {}

/// A root reached through a symlink, as `/var` is on macOS.
fn watch() -> Watch {
    Watch {
        events: mpsc::unbounded().1,
        root: PathBuf::from("/var/vault"),
        resolved: PathBuf::from("/private/var/vault"),
        watcher: Box::new(Unwatched),
    }
}

fn changed(path: impl Into<PathBuf>) -> Option<Changed> {
    watch().changed(&PathEvent {
        path: path.into(),
        rescan: false,
    })
}

fn rel(path: &str) -> RelPath {
    RelPath::new(Path::new(path)).expect("a relative path")
}

fn at(path: &str) -> Changed {
    Changed::Path(rel(path))
}

fn folders(paths: &[&str]) -> Pending {
    Pending::Folders(paths.iter().map(|path| rel(path)).collect())
}

#[test]
fn a_path_under_the_configured_root_counts() {
    assert_eq!(changed("/var/vault/f/a.md"), Some(at("f/a.md")));
}

#[test]
fn a_path_under_the_resolved_root_counts() {
    assert_eq!(changed("/private/var/vault/f/a.md"), Some(at("f/a.md")));
}

#[test]
fn the_root_itself_counts() {
    assert_eq!(changed("/private/var/vault"), Some(at("")));
}

#[test]
fn a_path_outside_both_roots_is_dropped() {
    assert_eq!(changed("/elsewhere/a.md"), None);
    assert_eq!(changed("/var/vaulted/a.md"), None, "a sibling, not a child");
}

#[test]
fn a_dotfile_or_anything_under_a_dot_folder_is_dropped() {
    assert_eq!(changed("/var/vault/.DS_Store"), None);
    assert_eq!(changed("/var/vault/.git/HEAD"), None);
    assert_eq!(changed("/var/vault/f/.obsidian/x.md"), None);
}

#[test]
fn a_root_inside_a_dot_folder_still_counts() {
    let watch = Watch {
        root: PathBuf::from("/home/.notes"),
        resolved: PathBuf::from("/home/.notes"),
        ..watch()
    };
    let event = PathEvent {
        path: PathBuf::from("/home/.notes/a.md"),
        rescan: false,
    };

    assert_eq!(watch.changed(&event), Some(at("a.md")));
}

#[test]
fn our_own_temporary_file_is_dropped() {
    assert_eq!(changed(temp_path(Path::new("/var/vault/f/a.md"))), None);
}

#[test]
fn a_file_that_only_looks_temporary_counts() {
    assert_eq!(changed("/var/vault/backup.tmp"), Some(at("backup.tmp")));
    assert_eq!(
        changed("/var/vault/a.inkra-draft.md"),
        Some(at("a.inkra-draft.md"))
    );
}

#[test]
fn a_rescan_event_asks_for_a_rescan() {
    let rescan = |path: &str| {
        watch().changed(&PathEvent {
            path: PathBuf::from(path),
            rescan: true,
        })
    };

    assert_eq!(rescan("/var/vault/f"), Some(Changed::Rescan));
    assert_eq!(
        rescan("/var/vault/.git"),
        None,
        "events lost in a hidden folder"
    );
}

#[test]
fn a_path_makes_its_folder_pending_and_not_itself() {
    let mut pending = Pending::default();

    pending.add(at("f/a.md"));
    pending.add(at("f/g"));

    assert_eq!(pending, folders(&["f"]));
}

#[test]
fn a_path_at_the_top_makes_the_root_folder_pending() {
    let mut pending = Pending::default();

    pending.add(at("a.md"));

    assert_eq!(pending, folders(&[""]));
}

#[test]
fn the_root_itself_is_everything() {
    let mut pending = Pending::default();

    pending.add(at(""));

    assert_eq!(pending, Pending::Everything);
}

#[test]
fn a_rescan_is_everything_and_stays_it() {
    let mut pending = folders(&["f"]);

    pending.add(Changed::Rescan);
    pending.add(at("g/b.md"));

    assert_eq!(pending, Pending::Everything);
}

#[test]
fn past_the_cap_the_folders_become_everything() {
    let mut pending = Pending::default();
    for n in 0..MAX_FOLDERS {
        pending.add(at(&format!("{n}/a.md")));
    }
    pending.add(at("0/b.md"));
    assert!(
        matches!(&pending, Pending::Folders(folders) if folders.len() == MAX_FOLDERS),
        "a second note of a pending folder adds nothing"
    );

    pending.add(at("last/a.md"));

    assert_eq!(pending, Pending::Everything);
}

#[test]
fn merging_unions_the_folders() {
    let mut pending = folders(&["", "f"]);

    pending.merge(folders(&["f", "g"]));

    assert_eq!(pending, folders(&["", "f", "g"]));
}

#[test]
fn merging_everything_either_way_is_everything() {
    let mut pending = folders(&["f"]);
    pending.merge(Pending::Everything);
    assert_eq!(pending, Pending::Everything);

    let mut pending = Pending::Everything;
    pending.merge(folders(&["f"]));
    assert_eq!(pending, Pending::Everything);
}

#[test]
fn merging_past_the_cap_is_everything() {
    let names = |from: usize, to: usize| -> BTreeSet<RelPath> {
        (from..to).map(|n| rel(&n.to_string())).collect()
    };
    let mut pending = Pending::Folders(names(0, MAX_FOLDERS));

    pending.merge(Pending::Folders(names(MAX_FOLDERS, MAX_FOLDERS + 1)));

    assert_eq!(pending, Pending::Everything);
}

#[test]
fn only_no_folders_is_empty() {
    assert!(Pending::default().is_empty());
    assert!(!folders(&[""]).is_empty());
    assert!(!Pending::Everything.is_empty());
}
