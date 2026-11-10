use std::path::Path;
use std::sync::Arc;

use gpui::{AppContext as _, TestAppContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::BacklinksPanel;
use super::tests::{one, panel, rows, show};
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
