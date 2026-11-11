use std::path::Path;
use std::sync::Arc;

use futures::StreamExt as _;
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div, prelude::FluentBuilder as _,
};
use inkra_index::READ_FAILURES;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, New};

use super::BacklinksPanel;
use super::job::READ_SOURCES;
use super::tests::{one, panel, redraws, rows, show};
use crate::index::{IndexSync, READERS};
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

#[gpui::test]
fn a_held_read_hides_no_row_and_no_other_context(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "[[b]] too");
    fs.insert_file("/vault/d.md", "and [[b]]");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    let gate = fs.block_load(A);

    show(cx, &notes, &panel, "b");

    assert_eq!(panel.read_with(cx, |panel, _| panel.count()), Some(3));
    assert_eq!(
        rows(cx, &panel),
        [
            ("a".to_owned(), vec![one("", "")]),
            ("c".to_owned(), vec![one("[[b]] too", "[[b]]")]),
            ("d".to_owned(), vec![one("and [[b]]", "[[b]]")]),
        ]
    );

    gate.release();
    cx.run_until_parked();

    assert_eq!(
        rows(cx, &panel)[0],
        ("a".to_owned(), vec![one("see [[b]]", "[[b]]")])
    );
}

#[gpui::test]
fn only_the_first_thousand_sources_are_read(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let source = |n: usize| format!("/vault/s{n:04}.md");
    for n in 0..=READ_SOURCES {
        fs.insert_file(source(n), "[[t]]");
    }
    fs.insert_file("/vault/t.md", "");
    let (notes, panel) = panel(cx, &fs);
    cx.run_until_parked();
    let (last_read, unread) = (source(READ_SOURCES - 1), source(READ_SOURCES));
    let loads = |fs: &MemFs| [&last_read, &unread].map(|path| fs.loads(Path::new(path)));
    let before = loads(&fs);

    show(cx, &notes, &panel, "t");

    assert_eq!(
        panel.read_with(cx, |panel, _| panel.count()),
        Some(READ_SOURCES + 1)
    );
    let after = loads(&fs);
    assert_eq!([after[0] - before[0], after[1] - before[1]], [1, 0]);
    let rows = rows(cx, &panel);
    assert_eq!(rows.last(), Some(&(format!("s{READ_SOURCES:04}"), vec![])));
}

/// One stop per note: a later job on the same note reads nothing, another note reads again.
#[gpui::test]
fn the_reads_stop_at_the_eighth_failure_until_another_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let source = |n: usize| format!("/vault/s{n:02}.md");
    // Past the failures, every read the stop can find in flight.
    let failing = READ_FAILURES + 2 * READERS + 1;
    fs.insert_file(A, "see [[t]]");
    for n in 0..failing {
        fs.insert_file(source(n), "[[t]]");
    }
    fs.insert_file("/vault/t.md", "");
    fs.insert_file("/vault/u.md", "");
    let (notes, panel) = panel(cx, &fs);
    let links = panel.read_with(cx, |panel, _| panel.links.clone());
    links.update(cx, |links, _| links.set_text_budget(0));
    cx.run_until_parked();
    for n in 0..failing {
        fs.make_unreadable(source(n));
    }
    let last = Path::new(&source(failing - 1)).to_owned();
    let loads = fs.loads(&last);

    show(cx, &notes, &panel, "t");

    assert_eq!(fs.loads(&last) - loads, 0);
    assert_eq!(
        rows(cx, &panel)[0],
        ("a".to_owned(), vec![one("see [[t]]", "[[t]]")])
    );

    fs.insert_file(A, "now [[t]] here");
    let a = entry_id(&notes, cx, "a");
    links.update(cx, |links, cx| links.stale(&[a], cx));
    cx.run_until_parked();

    assert_eq!(rows(cx, &panel)[0], ("a".to_owned(), vec![one("", "")]));

    show(cx, &notes, &panel, "u");
    show(cx, &notes, &panel, "t");

    assert_eq!(
        rows(cx, &panel)[0],
        ("a".to_owned(), vec![one("now [[t]] here", "[[t]]")])
    );
}

/// The panel drawn only once `shown`.
struct Host {
    panel: Entity<BacklinksPanel>,
    shown: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .when(self.shown, |host| host.child(self.panel.clone()))
    }
}

/// Finding the text cache full, a rebuild keeps the hidden job for the slots it did not fill.
#[gpui::test]
fn a_kept_job_reads_no_slot_a_rebuild_filled(cx: &mut TestAppContext) {
    const C: &str = "/vault/c.md";
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "see [[b]]");
    fs.insert_file("/vault/b.md", "");
    fs.insert_file(C, "[[b]] too");
    fs.insert_file("/vault/d.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    let vault = vault_of(&notes, cx);
    let window = cx.add_window(|_, cx| Host {
        panel: cx.new(|cx| BacklinksPanel::new(links.clone(), vault, cx)),
        shown: false,
    });
    let host = window.root(cx).expect("root view");
    let panel = host.read_with(cx, |host, _| host.panel.clone());
    links.update(cx, |links, _| links.set_text_budget(0));
    cx.run_until_parked();
    show(cx, &notes, &panel, "b");
    links.update(cx, |links, _| links.set_text_budget(1 << 20));
    let a = panel.read_with(cx, |panel, _| panel.rows[0].backlink.candidate().clone());
    let reader = links.read_with(cx, IndexSync::reader);
    let cached = cx
        .executor()
        .spawn(reader.stream(vec![a]).collect::<Vec<_>>());
    cx.run_until_parked();
    drop(cached);

    fs.insert_file("/vault/d.md", "changed");
    let d = entry_id(&notes, cx, "d");
    links.update(cx, |links, cx| links.stale(&[d], cx));
    cx.run_until_parked();
    links.update(cx, |links, _| links.set_text_budget(0));
    let loads = [A, C].map(|path| fs.loads(Path::new(path)));

    host.update(cx, |host, cx| {
        host.shown = true;
        cx.notify();
    });
    cx.run_until_parked();

    let after = [A, C].map(|path| fs.loads(Path::new(path)));
    assert_eq!([after[0] - loads[0], after[1] - loads[1]], [0, 1]);
    assert_eq!(
        rows(cx, &panel),
        [
            ("a".to_owned(), vec![one("see [[b]]", "[[b]]")]),
            ("c".to_owned(), vec![one("[[b]] too", "[[b]]")]),
        ]
    );
}
