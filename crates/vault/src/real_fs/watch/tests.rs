use std::path::{Path, PathBuf};

use notify::event::{Flag, ModifyKind};
use notify::{Event, EventKind};

use super::path_events;
use crate::watch::PathEvent;

const ROOT: &str = "/vault";

fn event(path: &str, rescan: bool) -> PathEvent {
    PathEvent {
        path: PathBuf::from(path),
        rescan,
    }
}

#[test]
fn every_path_of_an_event_is_sent_whatever_its_kind() {
    let raw = Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(PathBuf::from("/vault/a.md"))
        .add_path(PathBuf::from("/vault/b.md"));

    assert_eq!(
        path_events(Path::new(ROOT), Ok(raw)),
        [event("/vault/a.md", false), event("/vault/b.md", false)]
    );
}

#[test]
fn a_rescan_flag_marks_every_path() {
    let raw = Event::new(EventKind::Other)
        .add_path(PathBuf::from("/vault/f"))
        .set_flag(Flag::Rescan);

    assert_eq!(
        path_events(Path::new(ROOT), Ok(raw)),
        [event("/vault/f", true)]
    );
}

#[test]
fn a_rescan_with_no_path_rescans_the_root() {
    let raw = Event::new(EventKind::Other).set_flag(Flag::Rescan);

    assert_eq!(path_events(Path::new(ROOT), Ok(raw)), [event(ROOT, true)]);
}

#[test]
fn an_event_with_no_path_sends_nothing() {
    let raw = Event::new(EventKind::Any);

    assert_eq!(path_events(Path::new(ROOT), Ok(raw)), []);
}

/// The backend may have lost events: nothing tells which.
#[test]
fn an_error_rescans_the_root() {
    let error = notify::Error::generic("lost");

    assert_eq!(
        path_events(Path::new(ROOT), Err(error)),
        [event(ROOT, true)]
    );
}
