//! A read that disproves its stamp reloads its source.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use gpui::TestAppContext;
use inkra_vault::Mtime;
use inkra_vault::mem_fs::MemFs;

use super::tests::{one, panel, rows, show};
use crate::test_support::entry_id;

const A: &str = "/vault/a.md";

/// A source `a` of `b` in a vault no watch follows: only a read finds `a` changed behind the index.
fn unwatched() -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    fs.make_unwatchable("/vault");
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs
}

#[gpui::test]
fn a_source_rewritten_behind_the_index_shows_its_new_text(cx: &mut TestAppContext) {
    let fs = unwatched();
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    let revision = links.read_with(cx, |links, _| links.revision());
    fs.insert_file(A, "see [[b]] now");

    show(cx, &notes, &panel, "b");

    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("see [[b]] now", "[[b]]")])]
    );
    assert_eq!(
        links.read_with(cx, |links, _| links.revision()),
        revision + 1
    );
}

#[gpui::test]
fn a_source_gone_behind_the_index_leaves_the_rows(cx: &mut TestAppContext) {
    let fs = unwatched();
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    fs.remove(Path::new(A));

    show(cx, &notes, &panel, "b");

    assert_eq!(rows(cx, &panel), []);
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    assert!(!links.read_with(cx, |links, _| links.index().is_indexed(a)));
}

/// `c`'s first read is held; the restart after `a`'s reload may read `c` again unblocked.
#[gpui::test]
fn a_held_read_holds_no_stale(cx: &mut TestAppContext) {
    let fs = unwatched();
    fs.insert_file("/vault/c.md", "[[b]] too");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    fs.insert_file(A, "see [[b]] now");
    let gate = fs.block_load("/vault/c.md");

    show(cx, &notes, &panel, "b");

    assert_eq!(
        rows(cx, &panel)[0],
        ("a".to_owned(), vec![one("see [[b]] now", "[[b]]")])
    );
    drop(gate);
}

/// The cache holds `a`'s old text at its stamp: the reload must read the disk, not that text.
#[gpui::test]
fn a_rewrite_that_keeps_the_stamp_shows_the_new_text(cx: &mut TestAppContext) {
    // Sub-second, so the stamp is not racy (P8) and the read is cached.
    let mtime = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1) + Duration::from_millis(500));
    let fs = unwatched();
    fs.insert_with_mtime(A, "see [[b]]", mtime);
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    let a = entry_id(&notes, cx, "a");
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    fs.insert_with_mtime(A, "[[b]] see", mtime);

    links.update(cx, |links, cx| links.stale(&[a], cx));
    cx.run_until_parked();

    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("[[b]] see", "[[b]]")])]
    );
}

#[gpui::test]
fn an_unreadable_source_is_not_stale(cx: &mut TestAppContext) {
    let fs = unwatched();
    fs.insert_file("/vault/c.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    let (a, c) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "c"));
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    let revision = links.read_with(cx, |links, _| links.revision());
    let loads = fs.loads(Path::new(A));
    fs.make_unreadable(A);

    show(cx, &notes, &panel, "b");

    assert_eq!(fs.loads(Path::new(A)) - loads, 1, "the panel's one read");
    assert_eq!(links.read_with(cx, |links, _| links.revision()), revision);
    assert!(links.read_with(cx, |links, _| links.index().is_indexed(a)));
    links.update(cx, |links, cx| links.stale(&[c], cx));
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    assert_eq!(
        fs.loads(Path::new(A)) - loads,
        1,
        "not read again at its stamp"
    );
}
