//! `Scans`: one scan at a time, coalesced requests and batches, the initial scan's two passes (open-first spec, O2 and O6).

use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::{ROOT, run, vault, walked};
use super::*;
use crate::mem_fs::MemFs;
use crate::vault::test_helpers::{paths, rel, scan};

fn folders(names: &[&str]) -> Pending {
    Pending::Folders(names.iter().map(|name| rel(name)).collect())
}

fn listings(fs: &MemFs, folders: &[&str]) -> Vec<usize> {
    folders
        .iter()
        .map(|folder| fs.read_dirs(Path::new(folder)))
        .collect()
}

/// The initial scan's root pass: its swap and the full pass it returns.
fn root_pass(scans: &mut Scans, live: &mut Vault, fs: &MemFs) -> (Swapped, Scan) {
    let initial = scans
        .batch(Pending::Everything, live)
        .expect("an idle worker starts the scan");
    assert!(initial.is_initial());
    let listed = block_on(initial.list_root(fs));
    scans.swap_root(live, listed, &Claims::default())
}

#[test]
fn three_requests_during_a_scan_cost_one_more_scan() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();
    let listed = fs.read_dirs(Path::new(ROOT));

    let (mut first, running) = scans.request(&live);
    let running = running.expect("an idle worker starts the scan");
    let mut waiting: Vec<_> = (0..3)
        .map(|_| {
            let (wait, scan) = scans.request(&live);
            assert!(scan.is_none(), "one scan at a time");
            wait
        })
        .collect();

    let (swapped, next) = run(&mut scans, &mut live, running, &fs);
    assert!(swapped.replaced.is_ok());
    assert_eq!(first.try_recv(), Ok(Some(())));
    for wait in &mut waiting {
        assert_eq!(
            wait.try_recv(),
            Ok(None),
            "answered only by a scan that started after it"
        );
    }
    let (_, after) = run(&mut scans, &mut live, next.expect("one more scan"), &fs);
    assert!(after.is_none());
    for wait in &mut waiting {
        assert_eq!(wait.try_recv(), Ok(Some(())));
    }
    assert_eq!(fs.read_dirs(Path::new(ROOT)), listed + 2);
}

#[test]
fn requests_during_the_initial_scan_cost_one_more_scan() {
    let fs = vault(&["f/b.md"]);
    let mut live = Vault::new(Path::new(ROOT));
    let mut scans = Scans::default();
    let initial = scans
        .batch(Pending::Everything, &live)
        .expect("an idle worker starts the scan");
    let mut waiting: Vec<_> = (0..3)
        .map(|_| {
            let (wait, scan) = scans.request(&live);
            assert!(scan.is_none(), "one scan at a time");
            wait
        })
        .collect();

    let listed = block_on(initial.list_root(&fs));
    let (root, full) = scans.swap_root(&mut live, listed, &Claims::default());
    assert!(root.replaced.is_ok());
    assert!(!full.is_initial());
    let (_, next) = run(&mut scans, &mut live, full, &fs);
    for wait in &mut waiting {
        assert_eq!(wait.try_recv(), Ok(None), "the initial scan answers none");
    }
    let next = next.expect("one more scan");
    assert!(!next.is_initial(), "no second root pass");
    let (_, after) = run(&mut scans, &mut live, next, &fs);

    assert!(after.is_none());
    assert_eq!(
        fs.read_dirs(Path::new(ROOT)),
        2,
        "the root pass, then one more full scan"
    );
    for wait in &mut waiting {
        assert_eq!(wait.try_recv(), Ok(Some(())));
    }
}

#[test]
fn batches_during_a_scan_merge_into_one_scoped_scan() {
    let fs = vault(&["f/a.md", "g/b.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    for batch in [folders(&["f"]), folders(&["g"]), folders(&["f"])] {
        assert!(scans.batch(batch, &live).is_none(), "one scan at a time");
    }
    let scanned = walked(running.expect("an idle worker starts the scan"), &fs);
    fs.insert_file("/vault/f/c.md", "");
    fs.insert_file("/vault/g/e.md", "");
    let (_, next) = scans.swap(&mut live, scanned, &Claims::default());
    let names = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &names);
    let (_, after) = run(&mut scans, &mut live, next.expect("one scoped scan"), &fs);

    assert!(after.is_none());
    assert_eq!(
        listings(&fs, &names),
        [listed[0], listed[1] + 1, listed[2] + 1],
        "both folders, each once, and not the root"
    );
    assert_eq!(
        paths(&live),
        ["f", "f/a.md", "f/c.md", "g", "g/b.md", "g/e.md"]
    );
}

#[test]
fn a_batch_during_a_scan_that_a_request_follows_joins_the_request() {
    let fs = vault(&["f/a.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let (mut wait, none) = scans.request(&live);
    assert!(none.is_none());
    assert!(scans.batch(folders(&["f"]), &live).is_none());
    let (_, next) = run(&mut scans, &mut live, running.expect("started"), &fs);
    let listed = fs.read_dirs(Path::new(ROOT));
    let (_, after) = run(
        &mut scans,
        &mut live,
        next.expect("the request's scan"),
        &fs,
    );

    assert!(after.is_none(), "the batch starts no scan of its own");
    assert_eq!(
        fs.read_dirs(Path::new(ROOT)),
        listed + 1,
        "it reads everything"
    );
    assert_eq!(wait.try_recv(), Ok(Some(())));
}

#[test]
fn an_empty_batch_starts_no_scan() {
    let fs = vault(&["a.md"]);
    let live = scan(&fs);
    let mut scans = Scans::default();

    assert!(scans.batch(Pending::default(), &live).is_none());
    assert!(scans.request(&live).1.is_some(), "the worker stayed idle");
}

#[test]
fn an_incomplete_scoped_read_is_followed_by_one_full_scan() {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();

    fs.make_unreadable("/vault/f");
    let scoped = scans
        .batch(folders(&["f"]), &live)
        .expect("an idle worker starts the scan");
    let (swapped, next) = run(&mut scans, &mut live, scoped, &fs);
    assert!(swapped.replaced.is_ok());
    let names = ["/vault", "/vault/g"];
    let listed = listings(&fs, &names);
    let full = next.expect("a full scan follows the failed read");
    let (_, after) = run(&mut scans, &mut live, full, &fs);

    assert!(after.is_none(), "a full scan never asks for another");
    assert_eq!(listings(&fs, &names), [listed[0] + 1, listed[1] + 1]);
    assert_eq!(paths(&live), ["f", "f/b.md", "g", "g/c.md", "a.md"]);
}

/// O8 marks a failed initial scan scanned, yet no walk read the vault whole: a batch reads it whole.
#[test]
fn a_batch_after_a_failed_initial_scan_reads_the_whole_vault() {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    fs.make_unreadable(ROOT);
    let mut live = Vault::new(Path::new(ROOT));
    let mut scans = Scans::default();
    let (_, full) = root_pass(&mut scans, &mut live, &fs);
    let (_, next) = run(&mut scans, &mut live, full, &fs);
    assert!(next.is_none());
    assert!(paths(&live).is_empty());

    fs.make_unreadable("/nowhere");
    fs.insert_file("/vault/f/d.md", "");
    let scan = scans
        .batch(folders(&["f"]), &live)
        .expect("an idle worker starts the scan");
    run(&mut scans, &mut live, scan, &fs);

    assert_eq!(
        paths(&live),
        ["f", "f/b.md", "f/d.md", "g", "g/c.md", "a.md"]
    );
}

#[test]
fn the_initial_scan_lists_the_root_once() {
    let fs = vault(&["a.md", "f/b.md"]);
    let mut live = Vault::new(Path::new(ROOT));
    let mut scans = Scans::default();

    let (root, full) = root_pass(&mut scans, &mut live, &fs);
    assert!(root.notify);
    assert_eq!(paths(&live), ["f", "a.md"], "the root level first");
    assert!(!live.is_scanned());
    let (swapped, next) = run(&mut scans, &mut live, full, &fs);

    assert!(swapped.replaced.is_ok());
    assert!(next.is_none());
    assert_eq!(fs.read_dirs(Path::new(ROOT)), 1);
    assert_eq!(fs.read_dirs(Path::new("/vault/f")), 1);
    assert_eq!(paths(&live), ["f", "f/b.md", "a.md"]);
    assert!(live.is_scanned());
}

/// The full pass continues the root pass's listing, so together they read the vault whole.
#[test]
fn a_batch_after_the_initial_scan_reads_only_its_folders() {
    let fs = vault(&["a.md", "f/b.md"]);
    let mut live = Vault::new(Path::new(ROOT));
    let mut scans = Scans::default();
    let (_, full) = root_pass(&mut scans, &mut live, &fs);
    run(&mut scans, &mut live, full, &fs);

    let names = ["/vault", "/vault/f"];
    let listed = listings(&fs, &names);
    fs.insert_file("/vault/f/c.md", "");
    let scoped = scans
        .batch(folders(&["f"]), &live)
        .expect("an idle worker starts the scan");
    run(&mut scans, &mut live, scoped, &fs);

    assert_eq!(listings(&fs, &names), [listed[0], listed[1] + 1]);
    assert_eq!(paths(&live), ["f", "f/b.md", "f/c.md", "a.md"]);
}

#[test]
fn a_failed_root_pass_leaves_the_vault_unscanned_until_the_full_pass() {
    let fs = vault(&["a.md"]);
    fs.make_unreadable(ROOT);
    let mut live = Vault::new(Path::new(ROOT));
    let mut scans = Scans::default();

    let (root, full) = root_pass(&mut scans, &mut live, &fs);
    assert!(root.replaced.is_err());
    assert!(!root.notify);
    assert!(
        !live.is_scanned(),
        "O8 marks a failed full pass, not a failed root pass"
    );
    assert!(!full.is_initial());
    let (swapped, next) = run(&mut scans, &mut live, full, &fs);

    assert!(swapped.replaced.is_err());
    assert!(swapped.notify, "the vault turned scanned");
    assert!(live.is_scanned());
    assert!(next.is_none());
}
