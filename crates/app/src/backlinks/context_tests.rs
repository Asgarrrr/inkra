use std::path::Path;
use std::sync::Arc;

use gpui::{AppContext as _, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, New};

use super::BacklinksPanel;
use super::tests::{one, panel, redraws, rows, show};
use crate::test_support::{entry_id, index_sync, scan_notes, vault_of};

const A: &str = "/vault/a.md";

#[gpui::test]
fn a_panel_never_drawn_reads_no_source(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    let vault = vault_of(&notes, cx);
    let panel = cx.update(|cx| cx.new(|cx| BacklinksPanel::new(links, vault, cx)));
    cx.run_until_parked();
    let loads = fs.loads(Path::new(A));

    show(cx, &notes, &panel, "b");

    assert_eq!(fs.loads(Path::new(A)) - loads, 0);
    assert_eq!(rows(cx, &panel), [("a".to_owned(), vec![one("", "")])]);
}

#[gpui::test]
fn a_drawn_panel_reads_each_source_once(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "");
    let (notes, panel) = panel(cx, &fs);
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    // An uncached text: every read of `a` reaches the disk.
    links.update(cx, |links, _| links.set_text_budget(0));
    cx.run_until_parked();
    let loads = fs.loads(Path::new(A));

    show(cx, &notes, &panel, "b");

    assert_eq!(fs.loads(Path::new(A)) - loads, 1);
    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("see [[b]]", "[[b]]")])]
    );

    let c = entry_id(&notes, cx, "c");
    links.update(cx, |links, cx| links.stale(&[c], cx));
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new(A)) - loads, 1);
}

#[gpui::test]
fn an_edited_source_shows_its_new_context(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");

    fs.insert_file(A, "now [[b]] here");
    let a = entry_id(&notes, cx, "a");
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    links.update(cx, |links, cx| links.stale(&[a], cx));
    cx.run_until_parked();

    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("now [[b]] here", "[[b]]")])]
    );
}

/// A root `x.md` wins `[[x]]` over `d/x.md`, and the source is not reloaded.
#[gpui::test]
fn an_occurrence_that_moves_to_another_note_takes_its_context_away(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/s.md", "[[x]] [[d/x]]");
    fs.insert_file("/vault/d/x.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "x");
    assert_eq!(rows(cx, &panel)[0].1.len(), 2);

    let created = notes.update(cx, |notes, cx| notes.create(None, New::Note, "x", cx));
    cx.run_until_parked();
    drop(created);

    assert_eq!(
        rows(cx, &panel),
        [("s".to_owned(), vec![one("[[x]] [[d/x]]", "[[d/x]]")])]
    );
}

#[gpui::test]
fn a_note_shown_again_lands_its_cached_contexts_at_once(cx: &mut TestAppContext) {
    const C: &str = "/vault/c.md";
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file(C, "[[b]] too");
    fs.insert_file("/vault/d.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    let full = [
        ("a".to_owned(), vec![one("see [[b]]", "[[b]]")]),
        ("c".to_owned(), vec![one("[[b]] too", "[[b]]")]),
    ];
    assert_eq!(rows(cx, &panel), full);
    show(cx, &notes, &panel, "d");
    let loads = [A, C].map(|path| fs.loads(Path::new(path)));
    let b = entry_id(&notes, cx, "b");
    let (count, _guard) = redraws(cx, &panel);

    panel.update(cx, |panel, cx| panel.set_file(Some(b), cx));

    assert_eq!(rows(cx, &panel), full);
    assert_eq!(count.get(), 1);
    cx.run_until_parked();
    assert_eq!([A, C].map(|path| fs.loads(Path::new(path))), loads);
}
