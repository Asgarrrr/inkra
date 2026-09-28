//! The banner over a note deleted on disk (watcher spec, F9).

use std::path::Path;
use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext};
use inkra_vault::mem_fs::MemFs;

use super::{activate, two_notes};

const BANNER: &str = "gone-banner";

#[gpui::test]
fn the_banner_shows_only_while_the_note_showing_is_gone(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds(BANNER), None);

    fs.remove(Path::new("/vault/a.md"));
    visual.run_until_parked();
    assert!(visual.debug_bounds(BANNER).is_some(), "a is gone");

    fs.insert_file("/vault/a.md", "back");
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds(BANNER), None, "a is back");
}

#[gpui::test]
fn a_gone_note_not_showing_shows_no_banner_until_it_is_shown(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    activate(&sidebar, cx, b);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();

    fs.remove(Path::new("/vault/a.md"));
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds(BANNER), None, "b is showing");

    activate(&sidebar, &mut visual, a);
    visual.run_until_parked();
    assert!(visual.debug_bounds(BANNER).is_some(), "a is showing, gone");
}
