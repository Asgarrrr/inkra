//! A note deleted on disk stays open, gone; typing brings it back under its id (watcher spec, F9).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, Subscription, TestAppContext};
use inkra_text::Buffer;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::buffer_events::Reloaded;
use crate::test_support::{conflicts, edit, entry_id, index_sync, named, opened, scan_notes};
use crate::vault::store::AUTOSAVE_DELAY;
use crate::vault::{GoneChanged, NoteClosed, NoteStore, SaveFailed, TrashFailed};

const A: &str = "/vault/f/a.md";

/// `a` open and current, its text `alpha`.
async fn open_a(
    cx: &mut TestAppContext,
) -> (Arc<MemFs>, Entity<NoteStore>, EntryId, Entity<Buffer>) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;
    cx.run_until_parked();
    (fs, notes, a, buffer)
}

fn autosave(cx: &mut TestAppContext) {
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
}

fn is_gone(notes: &Entity<NoteStore>, cx: &mut TestAppContext, id: EntryId) -> bool {
    notes.read_with(cx, |notes, _| notes.is_gone(id))
}

/// The ids of every entry at `path`, relative to the vault.
fn ids_at(notes: &Entity<NoteStore>, cx: &mut TestAppContext, path: &str) -> Vec<EntryId> {
    notes.read_with(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .filter(|entry| entry.path.as_path() == Path::new(path))
            .map(|entry| entry.id)
            .collect()
    })
}

fn text(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> String {
    buffer.read_with(cx, |buffer, _| buffer.text().to_owned())
}

/// How many `Reloaded` the buffer emits.
fn reloads(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> (Rc<Cell<usize>>, Subscription) {
    let count = Rc::new(Cell::new(0));
    let subscription = cx.update(|cx| {
        let count = Rc::clone(&count);
        cx.subscribe(buffer, move |_, _: &Reloaded, _| count.set(count.get() + 1))
    });
    (count, subscription)
}

/// Every `GoneChanged` the store emits.
fn gone_events(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<GoneChanged>>>, Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let subscription = cx.update(|cx| {
        let events = Rc::clone(&events);
        cx.subscribe(notes, move |_, event: &GoneChanged, _| {
            events.borrow_mut().push(*event);
        })
    });
    (events, subscription)
}

#[gpui::test]
async fn a_clean_note_deleted_on_disk_stays_open_and_gone(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (reloaded, _reloads) = reloads(&buffer, cx);
    let (events, _events) = gone_events(&notes, cx);

    fs.remove(Path::new(A));
    cx.run_until_parked();

    assert!(is_gone(&notes, cx, a));
    assert_eq!(text(&buffer, cx), "alpha");
    assert_eq!(reloaded.get(), 0);
    assert_eq!(*events.borrow(), [GoneChanged(a)]);
    notes.read_with(cx, |notes, _| {
        assert_eq!(
            notes
                .sessions
                .get(a)
                .unwrap()
                .life
                .gone_path()
                .map(PathBuf::as_path),
            Some(Path::new(A)),
            "its last vault path"
        );
    });
}

#[gpui::test]
async fn a_dirty_note_deleted_on_disk_stays_open_and_gone(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (reloaded, _reloads) = reloads(&buffer, cx);
    let (events, _events) = gone_events(&notes, cx);
    edit(&buffer, cx, "X");

    fs.remove(Path::new(A));
    cx.run_until_parked();

    assert!(is_gone(&notes, cx, a));
    assert_eq!(text(&buffer, cx), "Xalpha");
    assert_eq!(reloaded.get(), 0);
    assert_eq!(*events.borrow(), [GoneChanged(a)]);
}

#[gpui::test]
async fn our_own_trash_closes_the_note_and_never_marks_it_gone(cx: &mut TestAppContext) {
    let (_fs, notes, a, _buffer) = open_a(cx).await;
    let (events, _events) = gone_events(&notes, cx);

    assert!(notes.update(cx, |notes, cx| notes.trash(a, cx)).await);
    cx.run_until_parked();

    assert!(notes.read_with(cx, |notes, _| notes.buffer_for(a).is_none()));
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
}

fn on_disk(fs: &MemFs) -> Option<String> {
    fs.read_to_string(Path::new(A))
}

#[gpui::test]
async fn typing_in_a_gone_note_recreates_it_at_its_last_path(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    fs.remove(Path::new("/vault/f"));
    cx.run_until_parked();
    assert!(is_gone(&notes, cx, a));

    let (reloaded, _reloads) = reloads(&buffer, cx);
    edit(&buffer, cx, "X");
    autosave(cx);

    assert_eq!(on_disk(&fs).as_deref(), Some("Xalpha"));
    assert!(!is_gone(&notes, cx, a), "the echo's swap reclaims the file");
    assert_eq!(entry_id(&notes, cx, "a"), a, "under the note's own id");
    assert_eq!(ids_at(&notes, cx, "f/a.md"), [a]);
    assert_eq!(reloaded.get(), 0);
    assert_eq!(text(&buffer, cx), "Xalpha");
}

/// The swap has not marked it gone yet: its save must fail, not recreate the folder beside a moved copy.
#[gpui::test]
async fn a_present_note_whose_folder_left_fails_its_save_until_it_is_gone(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let failed = Rc::new(Cell::new(0));
    let _failures = cx.update(|cx| {
        let failed = Rc::clone(&failed);
        cx.subscribe(&notes, move |_, _: &SaveFailed, _| {
            failed.set(failed.get() + 1);
        })
    });
    edit(&buffer, cx, "X");
    let scan = fs.block_read_dir("/vault");
    fs.remove(Path::new("/vault/f"));
    cx.run_until_parked();

    autosave(cx);
    assert!(!is_gone(&notes, cx, a), "the swap is held");
    assert_eq!(failed.get(), 1);
    assert!(futures::executor::block_on(fs.check_dir(Path::new("/vault/f"))).is_err());
    assert_eq!(on_disk(&fs), None);

    scan.release();
    cx.run_until_parked();
    assert!(is_gone(&notes, cx, a));
    edit(&buffer, cx, "Y");
    autosave(cx);
    assert_eq!(on_disk(&fs).as_deref(), Some("YXalpha"));
}

#[gpui::test]
async fn switching_away_from_an_untouched_gone_note_writes_nothing(cx: &mut TestAppContext) {
    let (fs, notes, a, _buffer) = open_a(cx).await;
    fs.remove(Path::new(A));
    cx.run_until_parked();

    let b = entry_id(&notes, cx, "b");
    opened(&notes, cx, b).await;
    autosave(cx);

    assert_eq!(on_disk(&fs), None);
    assert!(is_gone(&notes, cx, a));
}

#[gpui::test]
async fn the_quit_flush_writes_a_dirty_gone_note_at_its_last_path(cx: &mut TestAppContext) {
    let (fs, notes, _a, buffer) = open_a(cx).await;
    fs.remove(Path::new("/vault/f"));
    cx.run_until_parked();
    edit(&buffer, cx, "X");

    notes.update(cx, NoteStore::flush_all).await;

    assert_eq!(on_disk(&fs).as_deref(), Some("Xalpha"));
}

#[gpui::test]
async fn a_clean_gone_note_restored_outside_reloads_under_its_id(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    fs.remove(Path::new(A));
    cx.run_until_parked();
    let (events, _events) = gone_events(&notes, cx);

    fs.insert_file(A, "restored");
    cx.run_until_parked();

    assert!(!is_gone(&notes, cx, a));
    assert_eq!(*events.borrow(), [GoneChanged(a)]);
    assert_eq!(ids_at(&notes, cx, "f/a.md"), [a]);
    assert_eq!(text(&buffer, cx), "restored");
}

#[gpui::test]
async fn a_dirty_gone_note_restored_outside_keeps_its_text_and_preserves_theirs(
    cx: &mut TestAppContext,
) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (preserved, _conflicts) = conflicts(&notes, cx);
    fs.remove(Path::new(A));
    cx.run_until_parked();
    edit(&buffer, cx, "X");

    fs.insert_file(A, "restored");
    cx.run_until_parked();
    assert!(!is_gone(&notes, cx, a));
    assert_eq!(text(&buffer, cx), "Xalpha");

    autosave(cx);
    assert_eq!(on_disk(&fs).as_deref(), Some("Xalpha"));
    let preserved = preserved.borrow();
    assert_eq!(preserved.len(), 1, "{preserved:?}");
    assert_eq!(
        fs.read_to_string(&preserved[0]).as_deref(),
        Some("restored")
    );
}

/// `mv b.md f/a.md` over the open note: `b` is open, so its id is never reclaimed.
#[gpui::test]
async fn an_open_note_moved_over_a_gone_ones_path_keeps_its_id(cx: &mut TestAppContext) {
    let (fs, notes, a, _buffer) = open_a(cx).await;
    let b = entry_id(&notes, cx, "b");
    opened(&notes, cx, b).await;

    fs.remove(Path::new(A));
    futures::executor::block_on(fs.rename(Path::new("/vault/b.md"), Path::new(A)))
        .expect("move b over a");
    cx.run_until_parked();

    assert!(is_gone(&notes, cx, a));
    assert!(!is_gone(&notes, cx, b));
    assert_eq!(ids_at(&notes, cx, "f/a.md"), [b]);
}

/// `s` goes gone at `shared.md`; `a` is renamed onto it in the app, then deleted outside.
#[gpui::test]
async fn two_gone_notes_with_one_last_path_the_lower_id_reclaims_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/shared.md", "shared");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let (a, s) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "shared"));
    opened(&notes, cx, s).await;
    opened(&notes, cx, a).await;
    cx.run_until_parked();

    fs.remove(Path::new("/vault/shared.md"));
    cx.run_until_parked();
    assert!(
        notes
            .update(cx, |notes, cx| notes.rename(a, "shared", cx))
            .await
    );
    cx.run_until_parked();
    fs.remove(Path::new("/vault/shared.md"));
    cx.run_until_parked();
    assert!(is_gone(&notes, cx, a) && is_gone(&notes, cx, s));

    fs.insert_file("/vault/shared.md", "back");
    cx.run_until_parked();

    let (lower, higher) = (a.min(s), a.max(s));
    assert_eq!(ids_at(&notes, cx, "shared.md"), [lower]);
    assert!(!is_gone(&notes, cx, lower));
    assert!(is_gone(&notes, cx, higher));
}

#[gpui::test]
async fn quick_open_names_a_restored_gone_note_once_under_its_id(cx: &mut TestAppContext) {
    let (fs, notes, a, _buffer) = open_a(cx).await;
    let links = index_sync(&notes, cx);
    cx.run_until_parked();

    fs.remove(Path::new(A));
    cx.run_until_parked();
    assert_eq!(named(&links, cx, "a"), []);

    fs.insert_file(A, "restored");
    cx.run_until_parked();
    assert_eq!(named(&links, cx, "a"), [a]);
}

fn closed_events(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<NoteClosed>>>, Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let subscription = cx.update(|cx| {
        let events = Rc::clone(&events);
        cx.subscribe(notes, move |_, event: &NoteClosed, _| {
            events.borrow_mut().push(*event);
        })
    });
    (events, subscription)
}

enum Edited {
    Clean,
    Dirty,
}

/// Its unsaved text is discarded: the confirmation says so, and no copy is on disk to trash.
async fn trashing_a_gone_note_closes_it(cx: &mut TestAppContext, edited: Edited) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (closed, _closed) = closed_events(&notes, cx);
    fs.remove(Path::new(A));
    cx.run_until_parked();
    if let Edited::Dirty = edited {
        edit(&buffer, cx, "X");
    }

    assert!(notes.update(cx, |notes, cx| notes.trash(a, cx)).await);
    autosave(cx);

    assert!(notes.read_with(cx, |notes, _| notes.buffer_for(a).is_none()));
    assert_eq!(notes.read_with(cx, |notes, _| notes.current()), None);
    assert_eq!(*closed.borrow(), [NoteClosed(a)]);
    assert_eq!(on_disk(&fs), None);
    assert_eq!(fs.trashed(Path::new(A)), None);
}

#[gpui::test]
async fn trashing_a_clean_gone_note_closes_it(cx: &mut TestAppContext) {
    trashing_a_gone_note_closes_it(cx, Edited::Clean).await;
}

#[gpui::test]
async fn trashing_a_dirty_gone_note_closes_it_and_writes_nothing(cx: &mut TestAppContext) {
    trashing_a_gone_note_closes_it(cx, Edited::Dirty).await;
}

fn trash_failures(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
) -> (Rc<Cell<usize>>, Subscription) {
    let failed = Rc::new(Cell::new(0));
    let subscription = cx.update(|cx| {
        let failed = Rc::clone(&failed);
        cx.subscribe(notes, move |_, _: &TrashFailed, _| {
            failed.set(failed.get() + 1);
        })
    });
    (failed, subscription)
}

/// Removed outside while the trash waits on the preempted write: the trash finds no entry and still closes it.
#[gpui::test]
async fn a_note_removed_outside_while_its_trash_waits_still_closes(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (closed, _closed) = closed_events(&notes, cx);
    let (failed, _failures) = trash_failures(&notes, cx);
    edit(&buffer, cx, "X");
    let gate = fs.block_save(A);

    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    fs.remove(Path::new("/vault/f"));
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert!(trashed.await);
    assert!(notes.read_with(cx, |notes, _| notes.buffer_for(a).is_none()));
    assert_eq!(notes.read_with(cx, |notes, _| notes.current()), None);
    assert_eq!(*closed.borrow(), [NoteClosed(a)]);
    assert_eq!(failed.get(), 0);
}

/// Its own save recreated the file before the vault saw it: the trash removes it, or it returns as a new note.
#[gpui::test]
async fn trashing_a_gone_note_trashes_the_file_its_own_save_recreated(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx).await;
    let (closed, _closed) = closed_events(&notes, cx);
    let (failed, _failures) = trash_failures(&notes, cx);
    fs.remove(Path::new(A));
    cx.run_until_parked();
    let scan = fs.block_read_dir("/vault/f");
    edit(&buffer, cx, "X");
    autosave(cx);
    assert_eq!(on_disk(&fs).as_deref(), Some("Xalpha"));
    assert!(is_gone(&notes, cx, a), "the echo's swap is held");

    assert!(notes.update(cx, |notes, cx| notes.trash(a, cx)).await);
    scan.release();
    cx.run_until_parked();

    assert!(notes.read_with(cx, |notes, _| notes.buffer_for(a).is_none()));
    assert_eq!(*closed.borrow(), [NoteClosed(a)]);
    assert_eq!(failed.get(), 0);
    assert_eq!(on_disk(&fs), None);
    assert_eq!(fs.trashed(Path::new(A)).as_deref(), Some("Xalpha"));
}

/// A file that appeared at a gone note's path is not the note's: the trash closes the note and leaves the file.
#[gpui::test]
async fn trashing_a_gone_note_leaves_an_unrelated_file_at_its_path(cx: &mut TestAppContext) {
    let (fs, notes, a, _buffer) = open_a(cx).await;
    let (closed, _closed) = closed_events(&notes, cx);
    let (failed, _failures) = trash_failures(&notes, cx);
    fs.remove(Path::new(A));
    cx.run_until_parked();
    let scan = fs.block_read_dir("/vault/f");
    fs.insert_file(A, "unrelated");
    cx.run_until_parked();
    assert!(is_gone(&notes, cx, a), "the swap is held");

    assert!(notes.update(cx, |notes, cx| notes.trash(a, cx)).await);
    scan.release();
    cx.run_until_parked();

    assert!(notes.read_with(cx, |notes, _| notes.buffer_for(a).is_none()));
    assert_eq!(*closed.borrow(), [NoteClosed(a)]);
    assert_eq!(failed.get(), 0);
    assert_eq!(on_disk(&fs).as_deref(), Some("unrelated"));
    assert_eq!(fs.trashed(Path::new(A)), None);
}
