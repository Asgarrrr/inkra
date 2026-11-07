//! Rebuild index: the cache file removed in the writer's order, every note loaded again (P15).

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use futures::lock::Mutex;
use gpui::TestAppContext;
use inkra_index::persist::{MAX_BYTES, QUIET};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::IndexSync;
use super::cache_writer::{remove_in_order, write_in_order};
use super::cache_writer_tests::{CACHE, launch, vault};
use super::tests::saved_edit;

fn kind(fs: &MemFs) -> Option<ErrorKind> {
    block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES))
        .err()
        .and_then(|error| error.io_kind())
}

#[gpui::test]
async fn a_rebuild_removes_the_file_after_a_write_in_flight(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let (notes, links) = launch(cx, &fs);
    let _ = saved_edit(cx, &notes, "b", "zulu").await;
    let gate = fs.block_replace(CACHE);
    cx.executor().advance_clock(QUIET);
    cx.run_until_parked();

    links.update(cx, IndexSync::rebuild);
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert_eq!(kind(&fs), Some(ErrorKind::NotFound));
}

/// A removal that fails still moves the mark, so an older write cannot bring the old file back.
#[test]
fn a_failed_removal_keeps_an_older_write_out() {
    let fs = Arc::new(MemFs::new());
    let written = Arc::new(Mutex::new(0));
    let file = Path::new(CACHE);
    fs.insert_bytes(file, b"old".to_vec());
    fs.fail_next_removes(file, 1, ErrorKind::PermissionDenied);

    block_on(remove_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        2,
    ));
    block_on(write_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        1,
        || panic!("an older write ran after the removal"),
    ));

    assert_eq!(fs.read_to_string(file).as_deref(), Some("old"));
}

/// A rebuild before any write of this launch meets the mark at 0.
#[test]
fn the_first_removal_of_a_launch_removes_the_file() {
    let fs = Arc::new(MemFs::new());
    let file = Path::new(CACHE);
    fs.insert_bytes(file, b"old".to_vec());

    block_on(remove_in_order(Arc::default(), fs.clone(), file.into(), 0));

    assert_eq!(fs.read_to_string(file), None);
}
