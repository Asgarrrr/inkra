use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Entity, TestAppContext};
use inkra_text::ByteOffset;
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::test_support::{entry_id, scan_notes, two_notes};
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

use super::open_shell;

#[gpui::test]
async fn a_quit_flush_does_not_write_a_note_on_its_way_to_the_trash(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        cx.notify();
    });

    let gate = fs.block_save("/vault/a.md");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    let trashed = store.update(cx, |store, cx| store.trash(a, cx));

    let flush = store.update(cx, NoteStore::flush_all);
    // This second gate forces the losing order: without it, the flush's
    // write may never contend with the trash at all.
    let after = fs.block_save("/vault/a.md");
    gate.release();
    cx.run_until_parked();
    after.release();
    flush.await;
    cx.run_until_parked();

    assert!(trashed.await, "the delete must still happen");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "the flush must not write a note whose delete is already queued: \
         its write is not on the chain, so it lands after the trash and \
         recreates the file"
    );
}

#[gpui::test]
async fn a_quit_flush_overtakes_the_write_it_overlaps(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let gate = fs.block_save("/vault/a.md");

    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    visual.simulate_input("Y");
    // Held, never dropped: a dropped `Task` cancels the flush's write.
    let flush = notes.update(cx, NoteStore::flush_all);

    gate.release();
    flush.await;

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeXY"),
        "the flush carries the highest sequence, so it is the write that stands"
    );
    let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    assert_eq!(
        entries.len(),
        1,
        "the flush expects what the chain's write produced, not what the load returned"
    );
}

#[gpui::test]
async fn a_conflict_reaches_the_vault_tree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("the note opens");
    store.update(cx, |store, cx| store.set_current(a, cx));
    let before = store.read_with(cx, |store, cx| {
        store.vault().expect("a vault").read(cx).entries().count()
    });

    fs.insert_file("/vault/a.md", "theirs");
    // `Buffer` cannot notify for itself; without this the write never schedules.
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            buffer.insert(ByteOffset(0), "X");
        });
        cx.notify();
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    let after = store.read_with(cx, |store, cx| {
        store.vault().expect("a vault").read(cx).entries().count()
    });
    assert_eq!(after, before + 1, "the preserved file is listed");
}

#[gpui::test]
async fn an_ordinary_write_does_not_rescan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("the note opens");
    store.update(cx, |store, cx| store.set_current(a, cx));

    // An unreadable directory scans to nothing, so an unwanted rescan
    // here would empty the tree, not just leave it stale.
    fs.make_unreadable("/vault");
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| {
            buffer.insert(ByteOffset(0), "X");
        });
        cx.notify();
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("Xalpha")
    );
    let entries = store.read_with(cx, |store, cx| {
        store.vault().expect("a vault").read(cx).entries().count()
    });
    assert_eq!(entries, 2, "no rescan ran, so the tree is intact");
}

/// The only coverage of the write path calling `reidentify`; deleting it
/// would let that call rot without a failing test.
#[gpui::test]
fn a_save_keeps_the_note_reachable_across_a_rescan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    cx.run_until_parked();
    let opened = store.update(cx, |store, _| store.buffer_for(a).map(Entity::entity_id));

    store.update(cx, |store, cx| {
        let buffer = store.buffer_for(a).cloned().expect("a is open");
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset(0), "X");
            });
        });
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    let vault = store.update(cx, |store, _| store.vault().cloned().expect("a vault"));
    vault.update(cx, |vault, _| {
        futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
    });

    vault.update(cx, |vault, _| {
        assert!(
            vault.entry(a).is_some(),
            "the save moved the inode; without the re-key this id names nothing"
        );
    });
    let reopened = store.update(cx, |store, cx| store.open_note(a, cx));
    assert_eq!(
        futures::executor::block_on(reopened).map(|buffer| buffer.entity_id()),
        opened,
        "the row still opens the buffer that holds the edit"
    );
}

#[gpui::test]
fn quitting_writes_every_dirty_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, b) = two_notes(cx, &fs);

    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    store.update(cx, |store, cx| store.open_note(b, cx).detach());
    cx.run_until_parked();
    // Only `b` is current, so a flush that reused `save_current` would
    // write `b` and lose `a`.
    store.update(cx, |store, cx| store.set_current(b, cx));
    for id in [a, b] {
        store.update(cx, |store, cx| {
            let buffer = store.buffer_for(id).cloned().expect("the note is open");
            buffer.update(cx, |buffer, _| {
                buffer.transact_at(Instant::now(), |buffer| {
                    buffer.insert(ByteOffset(0), "EDITED ");
                });
            });
        });
    }

    cx.quit();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("EDITED alpha"),
        "the note that was not current must still be written"
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/b.md")).as_deref(),
        Some("EDITED beta")
    );
}

#[gpui::test]
fn quitting_leaves_a_clean_note_alone(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    // Text the buffer never held, so any write at all shows.
    fs.insert_file("/vault/a.md", "on disk, not in the buffer");
    fs.insert_file("/vault/b.md", "beta");
    let store = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&store, cx, "a");

    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    cx.run_until_parked();

    cx.quit();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("on disk, not in the buffer")
    );
}
