use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Instant, SystemTime};

use futures::FutureExt as _;
use futures::channel::oneshot;
use futures::executor::block_on;
use gpui::{AppContext as _, Entity, Subscription, TestAppContext};
use inkra_index::live::Status;
use inkra_index::{NoteQuery, TermHit};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind, Mtime, Serial, Stamp};

use super::{IndexSync, NotesChanged};
use crate::test_support::{entry_id, named, scan_notes};
use crate::vault::{AUTOSAVE_DELAY, NoteSaved, NoteStore};

pub(super) fn indexed_vault(
    cx: &mut TestAppContext,
    files: &[(&str, &str)],
) -> (Arc<MemFs>, Entity<NoteStore>, Entity<IndexSync>) {
    let fs = Arc::new(MemFs::new());
    for (name, text) in files {
        fs.insert_file(format!("/vault/{name}"), *text);
    }
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    (fs, notes, links)
}

/// The display names of the notes linking to `name`.
pub(super) fn backlinks(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    links: &Entity<IndexSync>,
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

pub(super) async fn saved_edit(
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

/// Every build started from now on waits until the returned sender fires or drops.
pub(super) fn hold_builds(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
) -> oneshot::Sender<()> {
    let (release, hold) = oneshot::channel();
    links.update(cx, |links, _| links.hold = Some(hold.shared()));
    release
}

/// Swaps in a rescan of `fs` that no create, rename or trash asked for, as an outside edit's would.
pub(super) fn rescan(cx: &mut TestAppContext, fs: &MemFs, notes: &Entity<NoteStore>) {
    let vault = notes.read_with(cx, |notes, _| notes.vault().expect("a vault").clone());
    vault.update(cx, |vault, cx| {
        futures::executor::block_on(vault.rescan(fs)).expect("a MemFs rescan");
        cx.notify();
    });
    cx.run_until_parked();
}

pub(super) fn create(cx: &mut TestAppContext, notes: &Entity<NoteStore>, name: &str) {
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, name, cx));
    cx.background_executor.run_until_parked();
    cx.run_until_parked();
    drop(created);
    cx.run_until_parked();
}

/// Counts `NotesChanged` while the subscription lives.
pub(super) fn notes_changed(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
) -> (Rc<Cell<usize>>, Subscription) {
    let count = Rc::new(Cell::new(0));
    let sink = Rc::clone(&count);
    let subscription = cx.update(|cx| {
        cx.subscribe(links, move |_, _: &NotesChanged, _| {
            sink.set(sink.get() + 1);
        })
    });
    (count, subscription)
}

pub(super) fn status(cx: &mut TestAppContext, links: &Entity<IndexSync>) -> Status {
    links.read_with(cx, |links, _| links.status())
}

pub(super) fn indexing(loaded: usize, total: usize) -> Status {
    Status::Indexing { loaded, total }
}

/// Each word start of `text`'s terms in the notes holding them all, with the hit's context.
pub(super) fn found(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
    text: &str,
) -> Vec<(EntryId, String)> {
    links.read_with(cx, |links, _| {
        let snapshot = links.index().snapshot();
        let query = NoteQuery::new(text, None, &[]);
        let cancel = AtomicBool::new(false);
        block_on(snapshot.search(&query, 100, &Serial, &cancel))
            .expect("not cancelled")
            .notes
            .into_iter()
            .flat_map(|group| {
                let note = group.note;
                group.hits.into_iter().map(move |hit| (note, hit.context))
            })
            .collect()
    })
}

#[gpui::test]
fn startup_indexes_every_note(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(
        cx,
        &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "[[b]] and [[B]]")],
    );
    assert_eq!(status(cx, &links), Status::Scanning);

    cx.run_until_parked();

    assert_eq!(status(cx, &links), Status::Ready);
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a", "c"]);
}

#[gpui::test]
fn the_notes_are_built_off_the_foreground_and_follow_a_rescan(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "see [[b]]"), ("b.md", "")]);
    assert_eq!(
        links.read_with(cx, |links, _| links.index().names().len()),
        0,
        "nothing is applied before the build runs"
    );
    cx.run_until_parked();
    assert_eq!(named(&links, cx, "b"), [entry_id(&notes, cx, "b")]);

    create(cx, &notes, "new");

    assert_eq!(named(&links, cx, "new"), [entry_id(&notes, cx, "new")]);
}

/// Every build applies; notifies during one start a single rebuild after it.
#[gpui::test]
fn a_build_landing_behind_the_vault_applies_then_rebuilds(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();
    let (changed, _subscription) = notes_changed(cx, &links);
    let first = hold_builds(cx, &links);
    create(cx, &notes, "c");
    let second = hold_builds(cx, &links);
    create(cx, &notes, "d");

    first.send(()).expect("the build waits");
    cx.run_until_parked();

    assert_eq!(named(&links, cx, "c"), [entry_id(&notes, cx, "c")]);
    assert!(named(&links, cx, "d").is_empty(), "read before d");
    assert_eq!(changed.get(), 1);

    second.send(()).expect("the rebuild waits");
    cx.run_until_parked();

    assert_eq!(named(&links, cx, "d"), [entry_id(&notes, cx, "d")]);
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a"]);
    assert_eq!(changed.get(), 2);
}

#[gpui::test]
fn status_goes_scanning_indexing_ready_as_notes_load(cx: &mut TestAppContext) {
    let (fs, _notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    assert_eq!(status(cx, &links), Status::Scanning);

    let gate = fs.block_load("/vault/a.md");
    cx.run_until_parked();
    assert_eq!(
        status(cx, &links),
        indexing(2, 3),
        "a is loading, b and c are loaded"
    );

    gate.release();
    cx.run_until_parked();
    assert_eq!(status(cx, &links), Status::Ready);
}

#[gpui::test]
fn a_rebuild_alone_is_not_building(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    let (changed, _changes) = notes_changed(cx, &links);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let _subscription = cx.update(|cx| {
        cx.observe(&links, move |links, cx| {
            let building = !links.read(cx).status().is_ready();
            sink.lock().expect("unpoisoned").push(building);
        })
    });

    let b = entry_id(&notes, cx, "b");
    let renamed = notes.update(cx, |notes, cx| notes.rename(b, "c", cx));
    cx.run_until_parked();
    drop(renamed);

    assert!(changed.get() > 0, "a rebuild applied");
    let seen = seen.lock().expect("unpoisoned");
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|building| !building), "{seen:?}");
}

#[gpui::test]
async fn a_write_replaces_the_notes_links(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();

    saved_edit(cx, &notes, "a", "now [[c]]").await;

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

/// The walk reads the first save; the second lands before the swap replays both re-keys.
#[gpui::test]
async fn two_saves_during_a_held_scan_reload_nothing_after_its_swap(cx: &mut TestAppContext) {
    let (fs, notes, _links) = indexed_vault(cx, &[("a.md", "alpha"), ("sub/b.md", "")]);
    cx.run_until_parked();
    let root = fs.block_read_dir("/vault");
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "c", cx));
    cx.run_until_parked();

    saved_edit(cx, &notes, "a", "one").await;
    let sub = fs.block_read_dir("/vault/sub");
    root.release();
    cx.run_until_parked();
    saved_edit(cx, &notes, "a", "two").await;
    let loads = fs.loads(Path::new("/vault/a.md"));
    sub.release();
    cx.run_until_parked();
    drop(created);

    assert_eq!(fs.loads(Path::new("/vault/a.md")), loads);
}

/// The test scheduler runs no spawned task until the test parks, so the build started first is held.
#[gpui::test]
fn a_save_during_a_build_is_not_reloaded_when_the_build_applies(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let vault = notes.read_with(cx, |notes, _| notes.vault().cloned().expect("a vault"));
    let path = Path::new("/vault/a.md");
    let before = futures::executor::block_on(fs.load(path))
        .expect("a loads")
        .stamp;
    let saved = futures::executor::block_on(fs.save(path, "saved", before))
        .expect("a saves")
        .stamp;
    let loads = fs.loads(path);

    vault.update(cx, |_, cx| cx.notify());
    notes.update(cx, |_, cx| {
        cx.emit(NoteSaved {
            id: a,
            text: Arc::from("saved"),
            stamp: saved,
        });
    });
    cx.run_until_parked();

    assert_eq!(fs.loads(path), loads);
    assert_eq!(found(cx, &links, "saved"), [(a, "saved".to_owned())]);
    rescan(cx, &fs, &notes);
    assert_eq!(fs.loads(path), loads, "the save's mtime is the one indexed");
}

/// A save during a held scan, then a write outside the app to the saved inode, which the walk reads.
#[gpui::test]
async fn an_in_place_write_the_held_walk_read_is_reloaded_after_its_swap(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "")]);
    cx.run_until_parked();
    let root = fs.block_read_dir("/vault");
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "c", cx));
    cx.run_until_parked();

    saved_edit(cx, &notes, "a", "saved").await;
    fs.insert_file("/vault/a.md", "outside");
    let loads = fs.loads(Path::new("/vault/a.md"));
    root.release();
    cx.run_until_parked();
    drop(created);

    // The index's reload, and the open note's (watcher spec, F8).
    assert_eq!(fs.loads(Path::new("/vault/a.md")), loads + 2);
    let a = entry_id(&notes, cx, "a");
    assert_eq!(found(cx, &links, "outside"), [(a, "outside".to_owned())]);
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
    assert!(!status(cx, &links).is_ready(), "the load is parked");

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
async fn a_load_and_a_save_put_their_text_in_the_corpus(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "loaded words"), ("b.md", "")]);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    assert_eq!(found(cx, &links, "words"), [(a, "loaded words".to_owned())]);

    saved_edit(cx, &notes, "b", "saved words").await;

    assert_eq!(
        found(cx, &links, "words"),
        [
            (a, "loaded words".to_owned()),
            (b, "saved words".to_owned())
        ]
    );
}

/// Passes against an `apply` that stores no text: the load test above pins the write.
#[gpui::test]
fn a_load_finishing_after_a_trash_leaves_no_text(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "old"), ("b.md", "")]);
    cx.run_until_parked();

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "c");
    assert!(!status(cx, &links).is_ready(), "the load is parked");
    let a = entry_id(&notes, cx, "a");
    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);
    gate.release();
    cx.run_until_parked();

    assert_eq!(found(cx, &links, "changed"), []);
    assert_eq!(found(cx, &links, "old"), []);
}

#[gpui::test]
fn a_failed_load_behind_a_newer_write_keeps_the_write(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "d");
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
    // Holds the batch this change starts, as its latency would: the vault still lists the note.
    let _batch = fs.block_read_dir("/vault");
    fs.remove(Path::new("/vault/a.md"));
    gate.release();
    cx.run_until_parked();

    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);
}

#[gpui::test]
fn a_note_that_fails_to_load_loses_its_old_links(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();

    // Removed between the walk that lists it and the load that reads it.
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "d");
    // Holds the batch this change starts, as its latency would: the vault still lists the note.
    let _batch = fs.block_read_dir("/vault");
    fs.remove(Path::new("/vault/a.md"));
    gate.release();
    cx.run_until_parked();

    let a = entry_id(&notes, cx, "a");
    assert!(!links.read_with(cx, |links, _| links.index().is_indexed(a)));
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

    saved_edit(cx, &notes, "a", "written").await;

    assert_eq!(*seen.lock().expect("unpoisoned"), ["written"]);
}

#[gpui::test]
fn a_store_without_a_vault_is_ready(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new()) as Arc<dyn Fs>;
    let notes = cx.update(|cx| cx.new(|cx| NoteStore::new(fs, None, cx)));
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));

    assert_eq!(status(cx, &links), Status::Ready);
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
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));

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
    assert_eq!(status(cx, &links), Status::Ready);
}

#[gpui::test]
fn a_load_in_flight_for_a_note_gone_is_not_counted_as_pending(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    assert_eq!(status(cx, &links), Status::Ready);

    // Edited outside the app, then read by the rescan a rename runs; the load parks.
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    let b = entry_id(&notes, cx, "b");
    let renamed = notes.update(cx, |notes, cx| notes.rename(b, "c", cx));
    cx.run_until_parked();
    drop(renamed);
    assert_eq!(
        status(cx, &links),
        indexing(1, 2),
        "a is loading, c is loaded"
    );

    let a = entry_id(&notes, cx, "a");
    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);

    assert_eq!(
        status(cx, &links),
        Status::Ready,
        "c, the only note, is loaded"
    );
    gate.release();
    cx.run_until_parked();
}

#[gpui::test]
fn a_note_reloading_during_its_load_counts_once(cx: &mut TestAppContext) {
    // One core, so `a`'s parked load holds the queue while it is queued again.
    cx.executor().set_num_cpus(1);
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[c]]");
    create(cx, &notes, "c");
    assert_eq!(
        status(cx, &links),
        indexing(1, 3),
        "a is loading, c is queued behind it"
    );

    fs.insert_file("/vault/a.md", "[[b]]");
    create(cx, &notes, "d");
    assert_eq!(
        status(cx, &links),
        indexing(1, 4),
        "a is loading and queued again, c and d are queued"
    );

    gate.release();
    cx.run_until_parked();
    assert_eq!(status(cx, &links), Status::Ready);
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a"]);
    assert_eq!(backlinks(cx, &notes, &links, "c"), Vec::<String>::new());
}

/// The corpus size tells an empty text from none.
type Snapshot = (
    Vec<(
        String,
        bool,
        Vec<(EntryId, usize)>,
        Option<String>,
        Vec<Option<TermHit>>,
    )>,
    usize,
);

/// Prefixes of every word the random texts hold.
const TERMS: [&str; 7] = ["a", "b", "c", "e", "n", "r", "zz"];

fn snapshot(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    links: &Entity<IndexSync>,
) -> Snapshot {
    let ids: Vec<(EntryId, String)> = notes.read_with(cx, |notes, cx| {
        let vault = notes.vault().expect("a vault").read(cx);
        vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| (entry.id, entry.path.as_path().display().to_string()))
            .collect()
    });
    links.read_with(cx, |links, _| {
        let index = links.index();
        let corpus = index.corpus();
        let words = index.words();
        let lookups: Vec<_> = TERMS
            .iter()
            .map(|term| {
                words
                    .lookup(&NoteQuery::new(term, None, &[]), &AtomicBool::new(false))
                    .expect("not cancelled")
            })
            .collect();
        let mut texts: HashMap<EntryId, String> = (0..corpus.len())
            .filter_map(|order| corpus.text(order))
            .map(|(id, text)| (id, text.to_owned()))
            .collect();
        let rows = ids
            .into_iter()
            .map(|(id, path)| {
                let backlinks = index
                    .backlinks(id)
                    .into_iter()
                    .map(|backlink| (backlink.source, backlink.links.len()))
                    .collect();
                let indexed = index.is_indexed(id);
                let text = indexed.then(|| texts.remove(&id).unwrap_or_default());
                let hits = lookups.iter().map(|lookup| lookup.get(0, id)).collect();
                (path, indexed, backlinks, text, hits)
            })
            .collect();
        (rows, corpus.heap_bytes())
    })
}

#[gpui::test(iterations = 100)]
async fn random_operations_leave_the_index_as_a_fresh_build(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(
        cx,
        &[
            ("a.md", "[[b]] [[n1]]"),
            ("b.md", "[[c]] [[a]]"),
            ("c.md", "[[r2]]"),
            ("d/e.md", "[[a]] [[e]]"),
        ],
    );
    let rng = cx.executor().rng();
    let pool = ["a", "b", "c", "e", "n1", "n2", "n3", "r1", "r2", "r3", "zz"];
    let text = || {
        (0..rng.random_range(0..3usize))
            .map(|_| format!("[[{}]]", pool[rng.random_range(0..pool.len())]))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut made = 0;
    for _ in 0..8 {
        if rng.random_bool(0.5) {
            cx.run_until_parked();
        }
        for _ in 0..rng.random_range(1..4usize) {
            let ids: Vec<(EntryId, PathBuf)> = notes.read_with(cx, |notes, cx| {
                let vault = notes.vault().expect("a vault").read(cx);
                vault
                    .entries()
                    .filter(|entry| entry.kind == Kind::Note)
                    .filter_map(|entry| Some((entry.id, vault.absolute(entry.id)?)))
                    .collect()
            });
            if ids.is_empty() {
                break;
            }
            let (id, path) = ids[rng.random_range(0..ids.len())].clone();
            made += 1;
            match rng.random_range(0..6u32) {
                // A note joins the vault and is written while its build may still run.
                5 => {
                    let name = format!("s{made}");
                    fs.insert_file(format!("/vault/{name}.md"), "");
                    let vault =
                        notes.read_with(cx, |notes, _| notes.vault().expect("a vault").clone());
                    vault.update(cx, |vault, cx| {
                        futures::executor::block_on(vault.rescan(&*fs)).expect("a MemFs rescan");
                        cx.notify();
                    });
                    let body = text();
                    saved_edit(cx, &notes, &name, &body).await;
                }
                0 => notes
                    .update(cx, |notes, cx| {
                        notes.create(None, Kind::Note, &format!("n{made}"), cx)
                    })
                    .detach(),
                1 => notes
                    .update(cx, |notes, cx| notes.rename(id, &format!("r{made}"), cx))
                    .detach(),
                2 => notes.update(cx, |notes, cx| notes.trash(id, cx)).detach(),
                3 => fs.insert_file(path, text()),
                _ => {
                    cx.run_until_parked();
                    let Some(name) = notes.read_with(cx, |notes, cx| {
                        let vault = notes.vault().expect("a vault").read(cx);
                        vault.entry(id).map(|entry| entry.path.display_name())
                    }) else {
                        continue;
                    };
                    let body = text();
                    saved_edit(cx, &notes, &name, &body).await;
                }
            }
        }
    }
    // A last rescan reads the edits made outside the app.
    notes
        .update(cx, |notes, cx| notes.create(None, Kind::Note, "last", cx))
        .detach();
    cx.run_until_parked();

    let fresh = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    cx.run_until_parked();

    assert_eq!(snapshot(cx, &notes, &links), snapshot(cx, &notes, &fresh));
    assert_eq!(status(cx, &links), Status::Ready);
    assert_eq!(
        links.read_with(cx, |links, _| links.index().names().len()),
        fresh.read_with(cx, |fresh, _| fresh.index().names().len())
    );
}

fn revision(cx: &mut TestAppContext, links: &Entity<IndexSync>) -> u64 {
    links.read_with(cx, |links, _| links.revision())
}

#[gpui::test]
fn a_build_applying_changes_the_revision_once(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();
    let before = revision(cx, &links);

    let a = entry_id(&notes, cx, "a");
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "z", cx));
    cx.run_until_parked();
    drop(renamed);

    assert_eq!(revision(cx, &links), before + 1, "one build, no load");
}

#[gpui::test]
fn a_failed_load_changes_the_revision(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", "")]);
    cx.run_until_parked();
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "d");
    cx.run_until_parked();
    let before = revision(cx, &links);

    // Holds the batch this change starts, as its latency would: the vault still lists the note.
    let _batch = fs.block_read_dir("/vault");
    fs.remove(Path::new("/vault/a.md"));
    gate.release();
    cx.run_until_parked();

    assert_eq!(revision(cx, &links), before + 1, "a's forget");
}
