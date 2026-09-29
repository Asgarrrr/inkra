//! The store keeps a session per id it touches, and walks only the open notes at a swap.

use std::collections::HashSet;
use std::path::Path;

use gpui::TestAppContext;
use inkra_vault::EntryId;

use crate::test_support::{entry_id, opened, scan_notes, vault};

fn ids(ids: &[EntryId]) -> HashSet<EntryId> {
    ids.iter().copied().collect()
}

#[gpui::test]
async fn a_swap_walks_only_the_open_notes(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md", "c.md", "d.md", "e.md", "f.md"]);
    let notes = scan_notes(cx, fs.clone());
    let mut id = |name| entry_id(&notes, cx, name);
    let (open, closed_trash, open_trash) = (id("a"), id("e"), id("f"));
    let renamed = [id("b"), id("c"), id("d")];
    opened(&notes, cx, open).await;
    opened(&notes, cx, open_trash).await;

    for (id, name) in renamed.into_iter().zip(["b2", "c2", "d2"]) {
        let done = notes.update(cx, |notes, cx| notes.rename(id, name, cx));
        assert!(done.await);
    }
    for id in [closed_trash, open_trash] {
        let done = notes.update(cx, |notes, cx| notes.trash(id, cx));
        assert!(done.await);
    }
    fs.insert_file(Path::new("/vault/a.md"), "theirs");
    cx.run_until_parked();

    notes.read_with(cx, |notes, _| {
        assert_eq!(
            notes.sessions.live_ids(),
            ids(&[open]),
            "the live set is the open note"
        );
        assert_eq!(
            notes.sessions.ids(),
            ids(&[open, closed_trash, open_trash]),
            "a renamed closed id is pruned; a trashed one stays, refusing a second trash"
        );
        let claims = notes.sessions.claims();
        assert_eq!(
            claims.open,
            ids(&[open]),
            "no dormant session keeps a reclaim off its id"
        );
        assert_eq!(
            claims.claimants.keys().copied().collect::<HashSet<_>>(),
            ids(&[open])
        );
    });
}

#[gpui::test]
async fn a_rename_queued_behind_another_keeps_the_chain(cx: &mut TestAppContext) {
    let fs = vault(&["b.md"]);
    let notes = scan_notes(cx, fs.clone());
    let b = entry_id(&notes, cx, "b");

    let first = notes.update(cx, |notes, cx| notes.rename(b, "b2", cx));
    let second = notes.update(cx, |notes, cx| notes.rename(b, "b3", cx));
    assert!(first.await);
    notes.read_with(cx, |notes, _| {
        assert_eq!(
            notes.sessions.ids(),
            ids(&[b]),
            "the second rename still waits on it"
        );
    });
    assert!(second.await);
    cx.run_until_parked();

    assert!(fs.read_to_string(Path::new("/vault/b3.md")).is_some());
    notes.read_with(cx, |notes, _| assert!(notes.sessions.ids().is_empty()));
}
