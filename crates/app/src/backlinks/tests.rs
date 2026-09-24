use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind};

use super::BacklinksPanel;
use crate::index::LinkSync;
use crate::test_support::{entry_id, scan_notes};
use crate::vault::NoteStore;

fn panel(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> (Entity<NoteStore>, Entity<BacklinksPanel>) {
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| LinkSync::new(&notes, cx)));
    let panel = cx.update(|cx| cx.new(|cx| BacklinksPanel::new(links, notes.clone(), cx)));
    (notes, panel)
}

fn show(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    panel: &Entity<BacklinksPanel>,
    name: &str,
) {
    let id = entry_id(notes, cx, name);
    panel.update(cx, |panel, cx| panel.set_note(Some(id), cx));
}

/// Each row as its source's name and every occurrence's (context, linked text).
fn rows(
    cx: &mut TestAppContext,
    panel: &Entity<BacklinksPanel>,
) -> Vec<(String, Vec<(String, String)>)> {
    panel.read_with(cx, |panel, _| {
        panel
            .rows
            .iter()
            .map(|row| {
                let occurrences = row
                    .occurrences
                    .iter()
                    .map(|o| (o.context.to_string(), o.context[o.link.clone()].to_owned()))
                    .collect();
                (row.name.to_string(), occurrences)
            })
            .collect()
    })
}

fn one(context: &str, lit: &str) -> (String, String) {
    (context.to_owned(), lit.to_owned())
}

#[gpui::test]
fn the_rows_are_the_sources_in_vault_order_with_each_occurrence(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "[[b]] and\n[[b|again]] here");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();

    show(cx, &notes, &panel, "b");

    assert_eq!(
        rows(cx, &panel),
        [
            ("a".to_owned(), vec![one("see [[b]]", "[[b]]")]),
            (
                "c".to_owned(),
                vec![
                    one("[[b]] and", "[[b]]"),
                    one("[[b|again]] here", "[[b|again]]")
                ]
            ),
        ]
    );
}

#[gpui::test]
fn no_note_and_no_backlinks_leave_no_rows(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();

    assert_eq!(rows(cx, &panel), []);
    show(cx, &notes, &panel, "a");
    assert_eq!(rows(cx, &panel), []);
    assert!(!panel.read_with(cx, |panel, _| panel.building));
}

#[gpui::test]
fn the_rows_follow_the_index_as_it_builds(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    let gate = fs.block_load("/vault/a.md");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");

    assert!(panel.read_with(cx, |panel, _| panel.building));
    assert_eq!(rows(cx, &panel), []);

    gate.release();
    cx.run_until_parked();

    assert!(!panel.read_with(cx, |panel, _| panel.building));
    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("[[b]]", "[[b]]")])]
    );
}

#[gpui::test]
fn a_renamed_source_shows_its_new_name(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");

    let a = entry_id(&notes, cx, "a");
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "z", cx));
    cx.run_until_parked();
    drop(renamed);

    assert_eq!(
        rows(cx, &panel),
        [("z".to_owned(), vec![one("[[b]]", "[[b]]")])]
    );
}

/// Throttled, not frozen: rows land while another note's load is still parked.
#[gpui::test]
fn rows_appear_while_the_index_is_still_building(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[c]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "");
    let gate = fs.block_load("/vault/b.md");
    let (notes, panel) = panel(cx, &fs);
    show(cx, &notes, &panel, "c");
    cx.run_until_parked();
    assert!(panel.read_with(cx, |panel, _| panel.building));

    cx.executor().advance_clock(super::BUILDING_REFRESH);
    cx.run_until_parked();

    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("[[c]]", "[[c]]")])]
    );
    gate.release();
}

/// Notifies of the panel: each one redraws the window, which reshapes the whole editor.
fn redraws(
    cx: &mut TestAppContext,
    panel: &Entity<BacklinksPanel>,
) -> (Rc<Cell<usize>>, gpui::Subscription) {
    let count = Rc::new(Cell::new(0));
    let sink = Rc::clone(&count);
    let subscription = cx.update(|cx| cx.observe(panel, move |_, _| sink.set(sink.get() + 1)));
    (count, subscription)
}

#[gpui::test]
fn an_index_change_that_leaves_the_rows_alone_redraws_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "unrelated");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    let (count, _guard) = redraws(cx, &panel);

    fs.insert_file("/vault/c.md", "still unrelated, [[elsewhere]]");
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "d", cx));
    cx.run_until_parked();
    drop(created);

    assert_eq!(
        count.get(),
        0,
        "c was reloaded, but b's backlinks are unchanged"
    );

    fs.insert_file("/vault/c.md", "now [[b]]");
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "e", cx));
    cx.run_until_parked();
    drop(created);

    assert!(count.get() >= 1, "c now links to b");
    assert_eq!(rows(cx, &panel).len(), 2);
}
