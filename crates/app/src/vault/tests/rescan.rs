use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Entity, TestAppContext};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;

use crate::test_support::{edit, opened, two_notes};
use crate::vault::store::AUTOSAVE_DELAY;

#[gpui::test]
async fn a_conflict_reaches_the_vault_tree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = opened(&store, cx, a).await;
    let before = store.read_with(cx, |store, cx| {
        store.vault().expect("a vault").read(cx).entries().count()
    });

    fs.insert_file("/vault/a.md", "theirs");
    edit(&buffer, cx, "X");
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

    let buffer = opened(&store, cx, a).await;

    // An unreadable directory scans to nothing, so an unwanted rescan
    // here would empty the tree, not just leave it stale.
    fs.make_unreadable("/vault");
    edit(&buffer, cx, "X");
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
