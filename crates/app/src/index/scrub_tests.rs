//! The scrub: the seeded notes re-read at the first `Ready`, a mismatch rejecting the cache (P16).

use std::io::ErrorKind;

use gpui::TestAppContext;
use inkra_index::live::Status;
use inkra_vault::EntryId;

use super::cache_writer_tests::{fine, launch, loads, vault};
use super::rebuild_tests::kind;
use super::tests::{found, status};
use crate::test_support::entry_id;

#[gpui::test]
fn a_cached_note_that_differs_from_its_file_rejects_the_cache(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    drop(launch(cx, &fs));
    cx.run_until_parked();
    // Same size, mtime and inode: only a parse of the file tells it from the cache.
    fs.insert_with_mtime("/vault/a.md", "bravo", fine(1));

    let (notes, links) = launch(cx, &fs);

    assert_eq!(
        loads(&fs),
        [3, 3],
        "the first launch, the scrub, the reload"
    );
    assert_eq!(found(cx, &links, "bravo"), [entry_id(&notes, cx, "a")]);
    assert_eq!(found(cx, &links, "alpha"), Vec::<EntryId>::new());
    assert_eq!(kind(&fs), Some(ErrorKind::NotFound));
}

#[gpui::test]
fn an_untouched_vault_keeps_the_cache(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha [[b]]"), ("b.md", "beta")]);
    drop(launch(cx, &fs));
    cx.run_until_parked();

    // The scrub reads `a.md` first and parks there: a rejected cache would load `b.md` again.
    let scrub = fs.block_load("/vault/a.md");
    let (_notes, links) = launch(cx, &fs);

    assert_eq!(
        loads(&fs),
        [2, 1],
        "the first launch, then the scrub's first read"
    );
    scrub.release();
    cx.run_until_parked();

    assert_eq!(loads(&fs), [2, 2], "then the scrub's read of b");
    assert_eq!(kind(&fs), None);
    assert_eq!(status(cx, &links), Status::Ready);
}
