//! The corpus `IndexSync` keeps: each note's stamp, under its id, across vault changes.

use std::path::Path;

use futures::executor::block_on;
use gpui::{Entity, TestAppContext};
use inkra_index::NoteQuery;
use inkra_index::live::Status;
use inkra_vault::{EntryId, Fs as _, Stamp};

use super::IndexSync;
use super::tests::{candidates, create, found, indexed_vault, saved_edit, status};
use crate::test_support::{entry_id, quick_open};

fn ranked(cx: &mut TestAppContext, links: &Entity<IndexSync>, text: &str) -> Vec<EntryId> {
    links.read_with(cx, |links, _| {
        let query = NoteQuery::new(text, None, &[]);
        quick_open(links.index(), &query, usize::MAX)
            .into_iter()
            .map(|found| found.id)
            .collect()
    })
}

/// Each candidate for `word` as its id, its path and its stamp.
fn stamped(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
    word: &str,
) -> Vec<(EntryId, String, Stamp)> {
    candidates(cx, links, word)
        .into_iter()
        .map(|candidate| (candidate.id, candidate.path.to_string(), candidate.stamp))
        .collect()
}

#[gpui::test]
fn a_rename_keeps_the_stamp(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha words"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let [(_, _, stamp)] = stamped(cx, &links, "alpha")[..] else {
        panic!("a holds alpha");
    };

    // `c` sorts after `b`, so the stamp changes slot.
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "c", cx));
    cx.run_until_parked();
    drop(renamed);

    assert_eq!(
        stamped(cx, &links, "alpha"),
        [(a, "c.md".to_owned(), stamp)]
    );
    assert_eq!(
        fs.loads(Path::new("/vault/c.md")),
        0,
        "a rename reloads nothing"
    );
}

#[gpui::test]
fn a_trash_drops_the_stamp(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "beta")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let beta = stamped(cx, &links, "beta");

    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);

    assert_eq!(found(cx, &links, "alpha"), []);
    assert_eq!(stamped(cx, &links, "beta"), beta);
    assert_eq!(
        links.read_with(cx, |links, _| links.index().corpus().len()),
        1
    );
}

#[gpui::test]
fn an_outside_edit_replaces_the_stamp_at_the_next_rescan(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "before"), ("b.md", "beta")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let (before, beta) = (stamped(cx, &links, "before"), stamped(cx, &links, "beta"));

    fs.insert_file("/vault/a.md", "after");
    assert_eq!(stamped(cx, &links, "before"), before, "not before a rescan");
    // `0` sorts first, so every older stamp changes slot in the same rescan.
    create(cx, &notes, "0");

    let on_disk = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("a loads")
        .stamp;
    assert_eq!(
        stamped(cx, &links, "after"),
        [(a, "a.md".to_owned(), on_disk)]
    );
    assert_eq!(found(cx, &links, "before"), []);
    assert_eq!(fs.loads(Path::new("/vault/b.md")), 1, "b is not reloaded");
    assert_eq!(
        stamped(cx, &links, "beta"),
        beta,
        "b keeps its stamp through the reorder"
    );
}

#[gpui::test]
fn status_and_the_corpus_agree_once_indexing_ends(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> = (0..12)
        .map(|n| (format!("n{n:02}.md"), format!("text {n}")))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_fs, _notes, links) = indexed_vault(cx, &files);
    cx.run_until_parked();

    assert_eq!(status(cx, &links), Status::Ready);
    assert_eq!(
        links.read_with(cx, |links, _| links.index().names().len()),
        12
    );
    assert_eq!(
        links.read_with(cx, |links, _| links.index().corpus().len()),
        12
    );
    assert_eq!(found(cx, &links, "text").len(), 12);
}

#[gpui::test]
async fn a_save_ranks_its_note_first_by_mtime(cx: &mut TestAppContext) {
    // `MemFs` stamps each write a second later, so `b` loads newer than `a`.
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "plan alpha"), ("b.md", "plan beta")]);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    assert_eq!(ranked(cx, &links, "plan"), [b, a], "the load's mtimes");

    // One length on both sides, so key 2 ties and the mtime decides.
    saved_edit(cx, &notes, "a", "plan gamma").await;

    assert_eq!(ranked(cx, &links, "plan"), [a, b], "the save's mtime");
}
