use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use gpui::{AppContext as _, Entity, TestAppContext};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind, Mtime, Stamp};

use super::LinkSync;
use crate::test_support::{entry_id, scan_notes};
use crate::vault::{AUTOSAVE_DELAY, NoteSaved, NoteStore};

fn indexed_vault(
    cx: &mut TestAppContext,
    files: &[(&str, &str)],
) -> (Arc<MemFs>, Entity<NoteStore>, Entity<LinkSync>) {
    let fs = Arc::new(MemFs::new());
    for (name, text) in files {
        fs.insert_file(format!("/vault/{name}"), *text);
    }
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| LinkSync::new(&notes, cx)));
    (fs, notes, links)
}

/// The display names of the notes linking to `name`.
fn backlinks(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    links: &Entity<LinkSync>,
    name: &str,
) -> Vec<String> {
    let note = entry_id(notes, cx, name);
    let sources: Vec<EntryId> = links.read_with(cx, |links, _| {
        links
            .index()
            .backlinks(note)
            .into_iter()
            .map(|backlink| backlink.source)
            .collect()
    });
    notes.read_with(cx, |notes, cx| {
        let vault = notes.vault().expect("a vault").read(cx);
        sources
            .iter()
            .map(|id| {
                let entry = vault.entry(*id).expect("a source is a note");
                entry.path.display_name()
            })
            .collect()
    })
}

async fn edit(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    name: &str,
    text: &str,
) -> Entity<Buffer> {
    let id = entry_id(notes, cx, name);
    let buffer = notes
        .update(cx, |notes, cx| notes.open_note(id, cx))
        .await
        .expect("the note opens");
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            let all = ByteOffset(0)..buffer.len();
            buffer.replace(all, text);
        });
        cx.notify();
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    buffer
}

fn create(cx: &mut TestAppContext, notes: &Entity<NoteStore>, name: &str) {
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, name, cx));
    cx.background_executor.run_until_parked();
    cx.run_until_parked();
    drop(created);
    cx.run_until_parked();
}

#[gpui::test]
fn startup_indexes_every_note(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(
        cx,
        &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "[[b]] and [[B]]")],
    );
    assert!(links.read_with(cx, |links, _| links.is_building()));

    cx.run_until_parked();

    assert!(!links.read_with(cx, |links, _| links.is_building()));
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a", "c"]);
}

#[gpui::test]
async fn a_write_replaces_the_notes_links(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();

    edit(cx, &notes, "a", "now [[c]]").await;

    assert_eq!(backlinks(cx, &notes, &links, "b"), Vec::<String>::new());
    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);

    let loads = fs.loads(Path::new("/vault/a.md"));
    create(cx, &notes, "d");
    assert_eq!(
        fs.loads(Path::new("/vault/a.md")),
        loads,
        "the rescan finds the mtime the write recorded"
    );
}

#[gpui::test]
fn a_created_note_takes_the_links_already_naming_it(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "[[new]]")]);
    cx.run_until_parked();

    create(cx, &notes, "new");

    assert_eq!(backlinks(cx, &notes, &links, "new"), ["a"]);
}

#[gpui::test]
fn a_rename_moves_backlinks_by_name_and_keeps_the_renamed_source(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "[[a]]")]);
    cx.run_until_parked();

    let b = entry_id(&notes, cx, "b");
    let renamed = notes.update(cx, |notes, cx| notes.rename(b, "c", cx));
    cx.run_until_parked();
    drop(renamed);

    assert_eq!(backlinks(cx, &notes, &links, "c"), Vec::<String>::new());
    assert_eq!(
        backlinks(cx, &notes, &links, "a"),
        ["c"],
        "c still links to a"
    );
    assert_eq!(
        fs.loads(Path::new("/vault/c.md")),
        0,
        "a rename reloads nothing"
    );
}

#[gpui::test]
fn a_trashed_note_takes_its_links_with_it(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();

    let a = entry_id(&notes, cx, "a");
    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);

    assert_eq!(backlinks(cx, &notes, &links, "b"), Vec::<String>::new());
}

#[gpui::test]
fn an_edit_made_outside_the_app_is_read_at_the_next_rescan(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();

    fs.insert_file("/vault/a.md", "[[c]]");
    assert_eq!(
        backlinks(cx, &notes, &links, "b"),
        ["a"],
        "not before a rescan"
    );
    create(cx, &notes, "d");

    assert_eq!(backlinks(cx, &notes, &links, "b"), Vec::<String>::new());
    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);
}

#[gpui::test]
fn a_load_finishing_after_a_newer_write_is_dropped(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(
        cx,
        &[
            ("a.md", "[[b]]"),
            ("b.md", ""),
            ("c.md", ""),
            ("stale.md", ""),
        ],
    );
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[stale]]");
    create(cx, &notes, "d");
    assert!(
        links.read_with(cx, |links, _| links.is_building()),
        "the load is parked"
    );

    notes.update(cx, |_, cx| {
        cx.emit(NoteSaved {
            id: a,
            text: Arc::from("[[c]]"),
            stamp: Stamp {
                mtime: Mtime(SystemTime::UNIX_EPOCH),
                size: 5,
            },
        });
    });
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);
    assert_eq!(backlinks(cx, &notes, &links, "stale"), Vec::<String>::new());
}

#[gpui::test]
fn a_note_that_fails_to_load_loses_its_old_links(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();

    // Removed between the walk that lists it and the load that reads it.
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "d");
    fs.remove(Path::new("/vault/a.md"));
    gate.release();
    cx.run_until_parked();

    let a = entry_id(&notes, cx, "a");
    assert!(!links.read_with(cx, |links, _| links.index().is_indexed(a)));
}

#[gpui::test]
fn a_note_queued_twice_is_loaded_once(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let b = Path::new("/vault/b.md");
    assert_eq!(fs.loads(b), 1);

    // The queue follows vault order, so `a` parks the worker before `b` is reached.
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/b.md", "[[a]]");
    create(cx, &notes, "d");
    fs.insert_file("/vault/b.md", "[[c]]");
    create(cx, &notes, "e");
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.loads(b), 2, "the first queue entry for b was superseded");
    assert_eq!(backlinks(cx, &notes, &links, "c"), ["b"]);
}

#[gpui::test]
async fn a_write_emits_the_text_it_wrote(cx: &mut TestAppContext) {
    let (_fs, notes, _links) = indexed_vault(cx, &[("a.md", "")]);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let _subscription = cx.update(|cx| {
        cx.subscribe(&notes, move |_, event: &NoteSaved, _| {
            sink.lock()
                .expect("unpoisoned")
                .push(event.text.to_string());
        })
    });

    edit(cx, &notes, "a", "written").await;

    assert_eq!(*seen.lock().expect("unpoisoned"), ["written"]);
}

#[gpui::test]
fn a_detached_note_emits_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/loose.md", "");
    let notes =
        cx.update(|cx| cx.new(|cx| NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None, cx)));
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
    let stamp = Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH),
        size: 0,
    };
    notes.update(cx, |notes, cx| {
        notes.adopt(buffer.clone(), PathBuf::from("/loose.md"), stamp, cx);
    });
    let seen = Arc::new(std::sync::Mutex::new(0));
    let sink = Arc::clone(&seen);
    let _subscription = cx.update(|cx| {
        cx.subscribe(&notes, move |_, _: &NoteSaved, _| {
            *sink.lock().expect("unpoisoned") += 1;
        })
    });
    let links = cx.update(|cx| cx.new(|cx| LinkSync::new(&notes, cx)));

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            buffer.insert(ByteOffset(0), "[[x]]");
        });
        cx.notify();
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/loose.md")).as_deref(),
        Some("[[x]]")
    );
    assert_eq!(*seen.lock().expect("unpoisoned"), 0);
    assert!(!links.read_with(cx, |links, _| links.is_building()));
}
