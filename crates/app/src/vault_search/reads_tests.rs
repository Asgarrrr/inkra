//! `⌘⇧F` through reads: one landing after every read, and which reads reload a note.

use std::path::Path;

use gpui::{Entity, TestAppContext};
use inkra_vault::EntryId;

use super::tests::{Opened, listed, open, search, with_texts};
use crate::index::IndexSync;
use crate::test_support::entry_id;

fn links(opened: &mut Opened) -> Entity<IndexSync> {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.links.clone())
}

fn revision(opened: &mut Opened) -> u64 {
    let links = links(opened);
    links.read_with(&opened.visual, |links, _| links.revision())
}

fn is_indexed(opened: &mut Opened, id: EntryId) -> bool {
    let links = links(opened);
    links.read_with(&opened.visual, |links, _| links.index().is_indexed(id))
}

/// The hit rows listed, headers left out.
fn hits(opened: &mut Opened) -> usize {
    listed(opened)
        .iter()
        .filter(|row| !row.starts_with('#'))
        .count()
}

#[gpui::test]
fn lands_once_after_every_read(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "plan"), ("c.md", "plan")]);
    let mut opened = open(cx, &fs);
    let gate = fs.block_load("/vault/c.md");

    search(&mut opened, "p l a n");
    assert_eq!(hits(&mut opened), 0, "the last read holds the answer");

    gate.release();
    opened.visual.run_until_parked();
    assert_eq!(hits(&mut opened), 3);
}

#[gpui::test]
fn unreadable_is_not_stale(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "other")]);
    let mut opened = open(cx, &fs);
    let a = entry_id(&opened.notes, cx, "a");
    let (path, revision_before) = (Path::new("/vault/a.md"), revision(&mut opened));
    let loads = fs.loads(path);
    fs.make_unreadable(path);

    search(&mut opened, "p l a n");

    assert_eq!(
        fs.loads(path) - loads,
        1,
        "the search's one read, no reload"
    );
    assert_eq!(hits(&mut opened), 0);
    assert_eq!(revision(&mut opened), revision_before);
    assert!(is_indexed(&mut opened, a));
}

#[gpui::test]
fn rewritten_reloads_through_a_load(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan")]);
    let mut opened = open(cx, &fs);
    fs.insert_file("/vault/a.md", "plan\nplan");
    let read = fs.block_load("/vault/a.md");
    search(&mut opened, "p l a n");
    let reload = fs.block_load("/vault/a.md");
    let revision_before = revision(&mut opened);

    read.release();
    opened.visual.run_until_parked();
    assert_eq!(hits(&mut opened), 2, "the text read now");
    assert_eq!(revision(&mut opened), revision_before, "the reload is held");

    reload.release();
    opened.visual.run_until_parked();
    assert_eq!(revision(&mut opened), revision_before + 1);
}

#[gpui::test]
fn removed_and_not_utf8_are_stale(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "plan"), ("c.md", "plan")]);
    let mut opened = open(cx, &fs);
    let ids = ["a", "b", "c"].map(|name| entry_id(&opened.notes, cx, name));
    fs.remove(Path::new("/vault/a.md"));
    fs.insert_bytes("/vault/b.md", [0xff, b'p']);

    search(&mut opened, "p l a n");

    assert_eq!(listed(&mut opened), ["# c (1)", "1: plan"]);
    let indexed = ids.map(|id| is_indexed(&mut opened, id));
    assert_eq!(
        indexed,
        [false, false, true],
        "a and b reloaded, and failed"
    );
}

#[gpui::test]
fn a_note_is_read_at_its_path_as_scanned(cx: &mut TestAppContext) {
    let file = "/vault/Notes/Cafe\u{301}.MD";
    let fs = with_texts(&[("Notes/Cafe\u{301}.MD", "plan")]);
    let mut opened = open(cx, &fs);
    let loads = fs.loads(Path::new(file));

    search(&mut opened, "p l a n");

    assert_eq!(hits(&mut opened), 1);
    assert_eq!(fs.loads(Path::new(file)) - loads, 1);
}
