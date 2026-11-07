//! A relaunch over the index cache file: read beside the scan, applied at the first build
//! (persisted index spec, P1, P7).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::{Entity, TestAppContext};
use inkra_index::live::Status;
use inkra_index::persist::encode;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Mtime};

use super::IndexSync;
use super::tests::{backlinks, found, status};
use crate::test_support::{cached_index_sync, entry_id, index_sync, scan_notes};
use crate::timeline::Timeline;
use crate::vault::NoteStore;

const CACHE: &str = "/cache/index.idx";

const FILES: [(&str, &str); 3] = [
    ("a.md", "alpha"),
    ("b.md", "[[a]] beta"),
    ("c.md", "[[a]] beta"),
];

/// `n` seconds and a half after the epoch: a sub-second mtime, so no stamp is racy (P8).
fn fine(n: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(n) + Duration::from_millis(500))
}

/// A launch over `FILES`, every note loaded, its index written to `CACHE`; nothing of it is left.
fn first_launch(cx: &mut TestAppContext) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for (n, (file, text)) in (1..).zip(FILES) {
        fs.insert_with_mtime(format!("/vault/{file}"), text, fine(n));
    }
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    cx.run_until_parked();
    let bytes = links.read_with(cx, |links, _| {
        encode(&links.index().snapshot(), Path::new("/vault"))
    });
    fs.insert_bytes(CACHE, bytes);
    drop((notes, links));
    cx.run_until_parked();
    fs
}

/// The next launch over the same disk; `0.md`, added first, shifts every `EntryId` by one.
fn relaunch(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> (Entity<NoteStore>, Entity<IndexSync>) {
    fs.insert_with_mtime("/vault/0.md", "zero beta", fine(10));
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cached_index_sync(&notes, Path::new(CACHE), cx);
    (notes, links)
}

/// The loads of `0.md`, `a.md`, `b.md` and `c.md`, over both launches.
fn loads(fs: &MemFs) -> [usize; 4] {
    ["0.md", "a.md", "b.md", "c.md"].map(|file| fs.loads(&Path::new("/vault").join(file)))
}

/// Every note read again, and the index whole.
fn reloads_every_note(cx: &mut TestAppContext, fs: &Arc<MemFs>) {
    let (notes, links) = relaunch(cx, fs);
    cx.run_until_parked();

    assert_eq!(loads(fs), [1, 2, 2, 2]);
    assert_eq!(status(cx, &links), Status::Ready);
    assert_eq!(backlinks(cx, &notes, &links, "a"), ["b", "c"]);
}

#[gpui::test]
fn a_relaunch_loads_only_the_note_the_cache_lacks(cx: &mut TestAppContext) {
    let fs = first_launch(cx);
    // The scrub reads `a.md` first and parks there: only the seed turn's loads are counted.
    let scrub = fs.block_load("/vault/a.md");
    let (notes, links) = relaunch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(
        loads(&fs),
        [1, 2, 1, 1],
        "0 at the seed turn, then the scrub's first read"
    );
    assert_eq!(status(cx, &links), Status::Ready);
    scrub.release();
    cx.run_until_parked();

    assert_eq!(loads(&fs), [1, 2, 2, 2], "the scrub's reads of a, b and c");
    assert_eq!(backlinks(cx, &notes, &links, "a"), ["b", "c"]);
    let beta = ["0", "b", "c"].map(|name| entry_id(&notes, cx, name));
    assert_eq!(found(cx, &links, "beta"), beta);
    assert_eq!(found(cx, &links, "alpha"), [entry_id(&notes, cx, "a")]);
}

#[gpui::test]
fn no_note_loads_before_the_cache_lands(cx: &mut TestAppContext) {
    let fs = first_launch(cx);
    let gate = fs.block_read_bytes(CACHE);
    let (_notes, links) = relaunch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(loads(&fs), [0, 1, 1, 1]);
    assert!(!status(cx, &links).is_ready());

    gate.release();
    cx.run_until_parked();

    assert_eq!(loads(&fs), [1, 2, 2, 2], "0, then the scrub's reads");
    assert_eq!(status(cx, &links), Status::Ready);
}

#[gpui::test]
fn a_missing_cache_reloads_every_note(cx: &mut TestAppContext) {
    let fs = first_launch(cx);
    fs.remove(Path::new(CACHE));

    reloads_every_note(cx, &fs);
}

#[gpui::test]
fn a_garbage_cache_reloads_every_note(cx: &mut TestAppContext) {
    let fs = first_launch(cx);
    fs.insert_bytes(CACHE, b"garbage".to_vec());

    reloads_every_note(cx, &fs);
}

#[gpui::test]
fn the_launch_line_names_the_cache_and_the_seed_turn(cx: &mut TestAppContext) {
    let fs = first_launch(cx);
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let _launched = relaunch(cx, &fs);
    cx.run_until_parked();

    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("cache decoded "), "{line}");
    let seed_turn = line.split_once("seed turn ").map(|(_, after)| after);
    assert!(
        seed_turn.is_some_and(|after| after.starts_with(|c: char| c.is_ascii_digit())),
        "{line}"
    );
}
