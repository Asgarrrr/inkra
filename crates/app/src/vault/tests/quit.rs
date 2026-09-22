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

/// The quit flush is the one writer that cannot join the chain, so it mints
/// the highest sequence a note will ever see. Without the `trashing` flag it
/// writes a note whose delete is still queued — the delete is then dropped
/// as stale and a confirmed deletion silently does not happen.
///
/// `flush_all` is called directly rather than through `cx.quit()`: the gate
/// holding the chain open has to be released from outside, and quitting does
/// not return until the flush completes.
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

    // Held open so the trash is still queued when the flush runs.
    let gate = fs.block_save("/vault/a.md");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    let trashed = store.update(cx, |store, cx| store.trash(a, cx));

    // Issued while the chain's write still holds the `Writer` lock, so the
    // flush's own write parks behind it rather than running now.
    let flush = store.update(cx, NoteStore::flush_all);
    // The second gate is what forces the losing order. Releasing the first
    // lets the chain's write finish and the trash run; the flush's write
    // then takes the lock and parks here, so when it finally lands the file
    // is already gone — and `MemFs::save` recreates a missing path.
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

/// The two writers of one note, genuinely concurrent — the interleaving
/// `Writer` exists for. The quit flush can't join the chain (`App::shutdown`
/// blocks while polling quit futures), so it issues its own write while the
/// chain's is still parked.
///
/// Driven through `flush_all`, not `cx.quit()`: the gate must release from
/// outside, and `quit` doesn't return until the flush finishes. (Wiring
/// through the real quit observer is `quitting_writes_every_dirty_note`'s
/// claim.)
///
/// Both assertions matter: a flush with its own `Writer`/stamp would
/// preserve our own text into a conflict file; a flush whose sequence didn't
/// exceed the chain's would be discarded, losing the newer edit.
#[gpui::test]
async fn a_quit_flush_overtakes_the_write_it_overlaps(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let gate = fs.block_save("/vault/a.md");

    // Parks the periodic chain inside `fs.save`, holding the note's lock:
    // `write_through` takes it before the write and keeps it across.
    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    // Text the parked write cannot be carrying, so which write landed last
    // is readable from the file alone.
    visual.simulate_input("Y");
    // Held, never dropped: the flush's writes are `Task`s moved into this
    // future, and dropping a `Task` cancels it.
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

/// The conflict file's only route to the user. Nothing else in the running
/// application rescans, so without this the file sits on disk unmentioned
/// until the next launch.
///
/// `two_notes`, not `open_shell`: the latter is the DETACHED path —
/// `NoteStore::new(fs, None, cx)` — and has no vault at all, so the rescan
/// branch is unreachable under it. This test needs no window; it asserts on
/// the vault's contents, not on a frame.
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
    // `update` plus an explicit `notify`: the observer `track` installs is
    // the autosave trigger, and `Buffer` cannot notify for itself. Without
    // the notify nothing schedules and the write never happens.
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

/// An ordinary write must not walk the directory. A conflict is rare;
/// `AUTOSAVE_DELAY` is not.
#[gpui::test]
async fn an_ordinary_write_does_not_rescan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("the note opens");
    store.update(cx, |store, cx| store.set_current(a, cx));

    // A scan of an unreadable directory yields nothing, so a rescan here
    // would empty the tree. Counting afterwards is what makes this test
    // about scanning rather than about the file's text.
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

/// The wiring that makes an atomic save survivable. `vault` proves
/// `reidentify` works when called; only this proves the write path calls
/// it. Nothing else in the `app` suite rescans, so without this the call
/// could be deleted and the workspace would stay green until the first
/// post-startup `rescan` caller orphans the open buffer.
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

    // A rescan is what create/rename/delete will trigger on every mutation
    // once they exist — deferred, with its trigger in the vault spec.
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

/// Every dirty note reaches disk on quit, not just the one `⌘S` would have
/// picked.
///
/// Quits via `TestAppContext::quit` → `App::shutdown`, not `cx.quit()` — see
/// CLAUDE.md, "Quit through `TestAppContext::quit`". Claims only that the
/// flush completes, not the 200 ms `SHUTDOWN_TIMEOUT`: under `TestScheduler`
/// the block's bound is a random tick count (`test_scheduler.rs:110-118`)
/// and the clock jumps to the next timer without consulting any deadline
/// (`:441-448`).
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

/// The version comparison, at quit. Without it every note on screen is
/// rewritten on every exit, which on a synced vault is a burst of
/// no-op changes the sync client still has to carry.
#[gpui::test]
fn quitting_leaves_a_clean_note_alone(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    // Text the buffer never held, so any write at all shows.
    fs.insert_file("/vault/a.md", "on disk, not in the buffer");
    fs.insert_file("/vault/b.md", "beta");
    // Not `two_notes`: it hardcodes "alpha" for `a.md`, and this test needs
    // text the buffer never held.
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
