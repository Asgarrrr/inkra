use std::path::Path;
use std::sync::Arc;

use gpui::TestAppContext;
use inkra_vault::mem_fs::MemFs;

use crate::test_support::{edit, opened, two_notes};
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

#[gpui::test]
async fn an_autosave_after_a_rename_lands_before_its_rescan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&store, cx, a).await;
    let gate = fs.block_read_dir("/vault");

    let renamed = store.update(cx, |store, cx| store.rename(a, "z", cx));
    cx.run_until_parked();
    edit(&buffer, cx, "X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/z.md")).as_deref(),
        Some("Xalpha"),
        "the autosave must not wait for the rename's rescan"
    );
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);

    gate.release();
    assert!(renamed.await);
}

#[gpui::test]
async fn the_quit_flush_during_a_rename_rescan_writes_the_new_name_only(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&store, cx, a).await;
    let gate = fs.block_read_dir("/vault");

    let renamed = store.update(cx, |store, cx| store.rename(a, "z", cx));
    cx.run_until_parked();
    edit(&buffer, cx, "X");
    let errors = store.update(cx, NoteStore::flush_all).await;

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/z.md")).as_deref(),
        Some("Xalpha")
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "the vault still lists a.md: the flush must not recreate it"
    );

    gate.release();
    assert!(renamed.await);
}

#[gpui::test]
async fn the_quit_flush_while_the_rename_runs_writes_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&store, cx, a).await;
    let gate = fs.block_after_rename("/vault/a.md");

    let renamed = store.update(cx, |store, cx| store.rename(a, "z", cx));
    cx.run_until_parked();
    edit(&buffer, cx, "X");
    let errors = store.update(cx, NoteStore::flush_all).await;

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "no path is safe while the rename runs"
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/z.md")).as_deref(),
        Some("alpha")
    );

    gate.release();
    assert!(renamed.await);
}

#[gpui::test]
async fn a_trash_behind_a_rename_trashes_the_new_name_before_its_rescan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    opened(&store, cx, a).await;
    let gate = fs.block_read_dir("/vault");

    let renamed = store.update(cx, |store, cx| store.rename(a, "z", cx));
    let trashed = store.update(cx, |store, cx| store.trash(a, cx));
    cx.run_until_parked();

    assert_eq!(
        fs.trashed(Path::new("/vault/z.md")).as_deref(),
        Some("alpha"),
        "the trash must not wait for the rename's rescan"
    );

    gate.release();
    assert!(renamed.await);
    assert!(trashed.await);
}

#[gpui::test]
async fn opening_a_renamed_note_before_its_rescan_loads_the_new_name(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    let gate = fs.block_read_dir("/vault");

    let renamed = store.update(cx, |store, cx| store.rename(a, "z", cx));
    cx.run_until_parked();
    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("the note opens at its new name");

    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text().to_owned()),
        "alpha"
    );

    gate.release();
    assert!(renamed.await);
}

#[gpui::test]
async fn two_renames_before_the_rescan_land_at_the_last_name(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    opened(&store, cx, a).await;
    let gate = fs.block_read_dir("/vault");

    let first = store.update(cx, |store, cx| store.rename(a, "z", cx));
    let second = store.update(cx, |store, cx| store.rename(a, "y", cx));
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert!(first.await);
    assert!(second.await);
    for gone in ["/vault/a.md", "/vault/z.md"] {
        assert_eq!(fs.read_to_string(Path::new(gone)), None, "{gone}");
    }
    assert_eq!(
        fs.read_to_string(Path::new("/vault/y.md")).as_deref(),
        Some("alpha")
    );
}

#[gpui::test]
async fn the_move_is_dropped_once_the_rename_rescan_lands(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);
    opened(&store, cx, a).await;
    let gate = fs.block_read_dir("/vault");

    drop(store.update(cx, |store, cx| store.rename(a, "z", cx)));
    gate.release();
    cx.run_until_parked();

    assert!(
        store.read_with(cx, |store, _| store.moves.is_empty()),
        "a dropped rename task must not skip the cleanup"
    );
}
