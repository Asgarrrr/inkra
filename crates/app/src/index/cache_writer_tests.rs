//! The index cache file written by one launch and read by the next (persisted index spec, P14).

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures::executor::block_on;
use futures::lock::Mutex;
use gpui::{Context, Entity, TestAppContext};
use inkra_index::persist::{MAX_BYTES, QUIET, Writes, encode};
use inkra_index::{FileRead, NoteContent};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{FileIdentity, Fs, Mtime, Stamp};

use super::IndexSync;
use super::cache_writer::write_in_order;
use super::tests::{found, saved_edit};
use crate::test_support::{cached_index_sync, entry_id, scan_notes};
use crate::vault::NoteStore;

pub(super) const CACHE: &str = "/cache/index.idx";

/// `n` seconds and a half after the epoch: a sub-second mtime, so no stamp is racy (P8).
fn fine(n: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(n) + Duration::from_millis(500))
}

pub(super) fn vault(files: &[(&str, &str)]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for (n, (file, text)) in (1..).zip(files) {
        fs.insert_with_mtime(format!("/vault/{file}"), text, fine(n));
    }
    fs
}

/// A launch over `fs` reading and writing `CACHE`, run until nothing is left to do.
pub(super) fn launch(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (Entity<NoteStore>, Entity<IndexSync>) {
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cached_index_sync(&notes, Path::new(CACHE), cx);
    cx.run_until_parked();
    (notes, links)
}

fn loads(fs: &MemFs) -> [usize; 2] {
    ["a.md", "b.md"].map(|file| fs.loads(&Path::new("/vault").join(file)))
}

#[gpui::test]
fn the_startup_writes_the_file_the_next_launch_reads(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let first = launch(cx, &fs);

    assert!(block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES)).is_ok());
    drop(first);
    cx.run_until_parked();
    let (notes, links) = launch(cx, &fs);

    assert_eq!(loads(&fs), [1, 1], "the first launch's loads only");
    assert_eq!(found(cx, &links, "beta"), [entry_id(&notes, cx, "b")]);
}

#[gpui::test]
fn a_launch_rewrites_the_file_only_when_its_seed_lost_a_note(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    drop(launch(cx, &fs));
    cx.run_until_parked();
    let mut writes = fs.watch(Path::new("/cache")).expect("a watch");

    drop(launch(cx, &fs));
    cx.run_until_parked();
    assert!(
        writes.events.try_recv().is_err(),
        "a seed that lost nothing writes nothing"
    );

    fs.remove(Path::new("/vault/a.md"));
    drop(launch(cx, &fs));
    assert!(
        writes.events.try_recv().is_ok(),
        "a pruned seed rewrites the file"
    );
}

#[gpui::test]
async fn an_edit_after_the_startup_is_written_after_the_quiet_window(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let first = launch(cx, &fs);
    let _ = saved_edit(cx, &first.0, "b", "zulu").await;
    cx.executor().advance_clock(QUIET);
    cx.run_until_parked();
    drop(first);
    cx.run_until_parked();

    let _held = fs.block_load("/vault/b.md");
    let (notes, links) = launch(cx, &fs);

    assert_eq!(found(cx, &links, "zulu"), [entry_id(&notes, cx, "b")]);
}

#[gpui::test]
async fn a_second_edit_restarts_the_quiet_window(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let (notes, _links) = launch(cx, &fs);
    let file = || block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES)).ok();
    let startup = file();
    let _ = saved_edit(cx, &notes, "b", "yankee").await;
    cx.executor().advance_clock(QUIET / 2);
    cx.run_until_parked();
    let _ = saved_edit(cx, &notes, "b", "zulu").await;

    cx.executor()
        .advance_clock(QUIET.saturating_sub(Duration::from_millis(1)));
    cx.run_until_parked();
    assert_eq!(file(), startup, "the first edit's window was restarted");

    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_ne!(file(), startup);
}

#[gpui::test]
async fn a_failed_write_keeps_the_old_file_and_retries_after_the_backoff(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let first = launch(cx, &fs);
    let file = || block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES)).ok();
    let startup = file();
    let _ = saved_edit(cx, &first.0, "b", "zulu").await;
    fs.fail_next_replaces(CACHE, 1, ErrorKind::StorageFull);

    cx.executor().advance_clock(QUIET);
    cx.run_until_parked();
    assert_eq!(file(), startup, "a failed write keeps the old file");
    cx.executor()
        .advance_clock(QUIET.saturating_sub(Duration::from_secs(1)));
    cx.run_until_parked();
    assert_eq!(file(), startup, "the backoff holds the retry");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_ne!(file(), startup);
    drop(first);
    cx.run_until_parked();

    let _held = fs.block_load("/vault/b.md");
    let (notes, links) = launch(cx, &fs);

    assert_eq!(found(cx, &links, "zulu"), [entry_id(&notes, cx, "b")]);
}

#[gpui::test]
async fn nothing_writes_the_file_at_quit(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let (notes, _links) = launch(cx, &fs);
    let file = || block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES)).ok();
    let startup = file();
    let _ = saved_edit(cx, &notes, "b", "zulu").await;

    cx.quit();
    cx.run_until_parked();

    assert_eq!(file(), startup);
}

/// The background runs two writes in flight in either order; each seed tries one.
#[gpui::test(iterations = 10)]
fn the_newer_of_two_writes_in_flight_lands(cx: &mut TestAppContext) {
    let fs = vault(&[("a.md", "alpha"), ("b.md", "beta")]);
    let (notes, links) = launch(cx, &fs);
    let b = entry_id(&notes, cx, "b");
    let file = FileRead {
        stamp: Stamp {
            mtime: fine(3),
            size: 4,
        },
        identity: FileIdentity {
            device: 1,
            inode: 1,
        },
    };

    let newer = links.update(cx, |sync, cx| {
        let write_now = |sync: &mut IndexSync, cx: &mut Context<IndexSync>| {
            sync.writer.as_mut().expect("a cache").writes = Some(Writes::new(None));
            sync.write_if_due(cx);
        };
        write_now(sync, cx);
        let ticket = sync.turn(cx, |turn| turn.saved(b, file.stamp));
        sync.turn(cx, |turn| {
            turn.landed(ticket, Ok(NoteContent::parse("zulu", file)));
        });
        write_now(sync, cx);
        encode(&sync.index().snapshot(), Path::new("/vault"))
    });
    cx.run_until_parked();

    assert_eq!(
        block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES)).ok(),
        Some(newer)
    );
}

/// A write that took the lock after a newer one encodes nothing and writes nothing.
#[test]
fn an_older_write_polled_last_is_never_encoded() {
    let fs = Arc::new(MemFs::new());
    let written = Arc::new(Mutex::new(0));
    let file = Path::new(CACHE);

    block_on(write_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        2,
        || b"new".to_vec(),
    ));
    block_on(write_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        1,
        || panic!("stale write encoded"),
    ));

    assert_eq!(fs.read_to_string(file).as_deref(), Some("new"));
}
