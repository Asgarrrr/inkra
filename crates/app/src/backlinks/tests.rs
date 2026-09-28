use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind};

use super::BacklinksPanel;
use crate::index::IndexSync;
use crate::test_support::{entry_id, scan_notes, unscanned_notes};
use crate::vault::NoteStore;

fn panel(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> (Entity<NoteStore>, Entity<BacklinksPanel>) {
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
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
    panel.read_with(cx, |panel, _| rows_of(panel))
}

fn rows_of(panel: &BacklinksPanel) -> Vec<(String, Vec<(String, String)>)> {
    panel
        .rows
        .iter()
        .map(|row| {
            let occurrences = row
                .occurrences
                .iter()
                .map(|o| {
                    let text = o.context.text();
                    (
                        text.to_string(),
                        text[o.context.ranges()[0].clone()].to_owned(),
                    )
                })
                .collect();
            (row.name.to_string(), occurrences)
        })
        .collect()
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

/// What `NoteStore::rescan` does once the trash's walk returns; the new build is not run.
fn trash_behind_the_store(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    fs: &Arc<MemFs>,
    path: &str,
) {
    fs.remove(std::path::Path::new(path));
    let Some(vault) = notes.read_with(cx, |notes, _| notes.vault().cloned()) else {
        panic!("a vault");
    };
    vault.update(cx, |vault, cx| {
        let rescan = futures::executor::block_on(vault.rescan(&**fs));
        assert!(rescan.is_ok(), "a MemFs rescan: {rescan:?}");
        cx.notify();
    });
}

/// The state between the vault's notify and the `NoteSet` build applying: at 50,000
/// notes the build takes about 105 ms.
#[gpui::test]
fn a_trashed_source_never_shows_as_a_nameless_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    assert_eq!(rows(cx, &panel).len(), 1);

    trash_behind_the_store(cx, &notes, &fs, "/vault/a.md");

    let names: Vec<String> = rows(cx, &panel).into_iter().map(|(name, _)| name).collect();
    assert!(names.iter().all(|name| !name.is_empty()), "{names:?}");
    cx.run_until_parked();
    assert_eq!(rows(cx, &panel), []);
}

/// `x.md` at the root wins `[[x]]` over `d/x.md`: the old rows must never take the new name.
#[gpui::test]
fn a_rename_never_shows_a_stale_or_nameless_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[x]]");
    fs.insert_file("/vault/c.md", "[[x]]");
    fs.insert_file("/vault/d/x.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "x");
    let before = vec![
        ("a".to_owned(), vec![one("[[x]]", "[[x]]")]),
        ("c".to_owned(), vec![one("[[x]]", "[[x]]")]),
    ];
    let after = Vec::new();
    assert_eq!(rows(cx, &panel), before);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let _guard = cx.update(|cx| {
        cx.observe(&panel, move |panel, cx| {
            sink.borrow_mut().push(rows_of(panel.read(cx)));
        })
    });

    let a = entry_id(&notes, cx, "a");
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "x", cx));
    cx.run_until_parked();
    drop(renamed);

    let seen = seen.borrow();
    assert!(
        seen.iter().all(|rows| *rows == before || *rows == after),
        "{seen:?}"
    );
    assert_eq!(rows(cx, &panel), after);
}

/// A load's notify can land after the vault changed and before the build of it applies.
#[gpui::test]
fn a_source_gone_from_the_vault_is_skipped_until_its_build_applies(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "[[b]]");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    assert_eq!(rows(cx, &panel).len(), 2);

    trash_behind_the_store(cx, &notes, &fs, "/vault/a.md");
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    links.update(cx, |_, cx| cx.notify());
    assert_eq!(
        rows(cx, &panel).len(),
        2,
        "a notify that changed nothing keeps the rows of the last applied build"
    );

    links.update(cx, IndexSync::simulate_load);
    assert_eq!(
        rows(cx, &panel),
        [("c".to_owned(), vec![one("[[b]]", "[[b]]")])]
    );
}

#[gpui::test]
fn an_applied_build_recomputes_the_rows_once(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    let before = panel.read_with(cx, |panel, _| panel.rebuilds);

    let c = entry_id(&notes, cx, "c");
    let renamed = notes.update(cx, |notes, cx| notes.rename(c, "d", cx));
    cx.run_until_parked();
    drop(renamed);

    let after = panel.read_with(cx, |panel, _| panel.rebuilds);
    assert_eq!(
        after - before,
        1,
        "NotesChanged and the notify after it are one change"
    );
}

#[gpui::test]
fn an_indented_link_drops_its_indent_and_draws_a_tab_as_a_space(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    // Two spaces: a leading tab or four spaces would open a code block, which holds no link.
    fs.insert_file("/vault/a.md", "  see\t[[b]]");
    fs.insert_file("/vault/b.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();

    show(cx, &notes, &panel, "b");

    assert_eq!(
        rows(cx, &panel),
        [("a".to_owned(), vec![one("see [[b]]", "[[b]]")])]
    );
}

#[gpui::test]
fn a_newline_in_a_source_name_draws_as_a_space(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a\nb.md", "[[c]]");
    fs.insert_file("/vault/c.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    show(cx, &notes, &panel, "c");

    assert_eq!(
        panel.read_with(cx, |panel, _| panel.source_names()),
        ["a b"]
    );
}

/// Before the first build no note has backlinks yet: "Indexing…", never "No backlinks".
#[gpui::test]
fn a_note_opened_during_the_initial_scan_shows_building(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "[[a]]");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    let panel = cx.update(|cx| cx.new(|cx| BacklinksPanel::new(links, notes.clone(), cx)));
    cx.run_until_parked();

    show(cx, &notes, &panel, "a");

    assert!(panel.read_with(cx, |panel, _| panel.building));
    gate.release();
}
