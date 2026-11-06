//! `Reader`: the candidates' order kept, at most `READERS` in flight, each failure's read, the
//! text cache.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use futures::{FutureExt as _, StreamExt as _};
use gpui::{Entity, TestAppContext};
use inkra_index::{Candidate, NoteQuery, Read, TextSearch};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::IndexSync;
use super::reads::READERS;
use super::tests::{candidates, hold_builds, indexed_vault, saved_edit};
use crate::test_support::entry_id;

/// Each candidate's read, in the order the stream yields them, collected off the foreground.
fn read_all(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
    candidates: Vec<Candidate>,
) -> gpui::Task<Vec<(Candidate, Read)>> {
    let reader = links.read_with(cx, IndexSync::reader);
    cx.background_executor
        .spawn(async move { reader.stream(candidates).collect().await })
}

/// `n` notes holding `alpha`, loaded.
fn alphas(cx: &mut TestAppContext, n: usize) -> (Arc<MemFs>, Entity<IndexSync>, Vec<String>) {
    let files: Vec<String> = (0..n).map(|n| format!("n{n}.md")).collect();
    let texts: Vec<(&str, &str)> = files.iter().map(|file| (file.as_str(), "alpha")).collect();
    let (fs, _notes, links) = indexed_vault(cx, &texts);
    cx.run_until_parked();
    let paths = files.iter().map(|file| format!("/vault/{file}")).collect();
    (fs, links, paths)
}

fn loads(fs: &MemFs, paths: &[String]) -> usize {
    paths.iter().map(|path| fs.loads(Path::new(path))).sum()
}

/// The notes a search for `word` hits, read through a reader taken now, as `⌘⇧F` does.
fn search(cx: &mut TestAppContext, links: &Entity<IndexSync>, word: &str) -> Vec<EntryId> {
    let candidates = candidates(cx, links, word);
    let reads = read_all(cx, links, candidates);
    cx.run_until_parked();
    let query = NoteQuery::new(word, None, &[]);
    let cancel = AtomicBool::new(false);
    let mut search = TextSearch::new(&query, usize::MAX);
    for (candidate, read) in reads.now_or_never().expect("every read landed") {
        search.read(&candidate, read, &cancel);
    }
    let found = search.finish(&cancel).expect("not cancelled");
    found
        .searched
        .notes
        .iter()
        .map(|group| group.note)
        .collect()
}

#[gpui::test]
fn reads_keep_the_candidates_order(cx: &mut TestAppContext) {
    let (fs, links, paths) = alphas(cx, 3);
    let candidates = candidates(cx, &links, "alpha");
    let gate = fs.block_load(paths[0].as_str());
    let before = loads(&fs, &paths);

    let mut reads = read_all(cx, &links, candidates.clone());
    cx.run_until_parked();
    assert_eq!(
        loads(&fs, &paths) - before,
        3,
        "the later reads ran meanwhile"
    );
    assert!(
        (&mut reads).now_or_never().is_none(),
        "the first read holds the rest"
    );

    gate.release();
    cx.run_until_parked();
    let reads = reads.now_or_never().expect("every read landed");
    let order: Vec<Candidate> = reads.into_iter().map(|(candidate, _)| candidate).collect();
    assert_eq!(order, candidates);
}

#[gpui::test]
fn at_most_four_reads_run_at_once(cx: &mut TestAppContext) {
    let (fs, links, paths) = alphas(cx, READERS + 2);
    let candidates = candidates(cx, &links, "alpha");
    let mut gates: Vec<_> = paths
        .iter()
        .map(|path| fs.block_load(path.as_str()))
        .collect();
    let before = loads(&fs, &paths);

    let _reads = read_all(cx, &links, candidates);
    cx.run_until_parked();
    assert_eq!(loads(&fs, &paths) - before, READERS);

    gates.remove(0).release();
    cx.run_until_parked();
    assert_eq!(
        loads(&fs, &paths) - before,
        READERS + 1,
        "a landed read frees its place"
    );
}

#[gpui::test]
fn each_outcome_maps_to_its_read(cx: &mut TestAppContext) {
    let (fs, links, paths) = alphas(cx, 4);
    let candidates = candidates(cx, &links, "alpha");
    fs.remove(Path::new(&paths[1]));
    fs.insert_bytes(paths[2].as_str(), [0xff, b'a']);
    fs.make_unreadable(paths[3].as_str());

    let reads = read_all(cx, &links, candidates.clone());
    cx.run_until_parked();

    let reads: Vec<Read> = reads
        .now_or_never()
        .expect("every read landed")
        .into_iter()
        .map(|(_, read)| read)
        .collect();
    let text = Read::Text {
        text: "alpha".into(),
        stamp: candidates[0].stamp,
    };
    assert_eq!(reads, [text, Read::Gone, Read::NotUtf8, Read::Failed]);
}

#[gpui::test]
fn second_search_reads_nothing(cx: &mut TestAppContext) {
    let (fs, links, paths) = alphas(cx, 3);
    search(cx, &links, "alpha");
    let each =
        |fs: &MemFs| -> Vec<usize> { paths.iter().map(|path| fs.loads(Path::new(path))).collect() };
    let before = each(&fs);

    assert_eq!(search(cx, &links, "alpha").len(), 3);
    assert_eq!(each(&fs), before);
}

#[gpui::test]
fn old_stamp_not_served(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha old")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    assert_eq!(search(cx, &links, "alpha"), [a]);

    fs.insert_file("/vault/a.md", "alpha newer");
    cx.run_until_parked();

    assert_eq!(search(cx, &links, "newer"), [a], "the new text is read");
    assert_eq!(search(cx, &links, "old"), []);
}

#[gpui::test]
fn a_read_at_another_stamp_is_not_cached(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha old")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let release = hold_builds(cx, &links);
    fs.insert_file("/vault/a.md", "alpha newer");
    cx.run_until_parked();
    assert_eq!(search(cx, &links, "alpha"), [a], "read ahead of its reload");

    drop(release);
    cx.run_until_parked();
    let before = fs.loads(Path::new("/vault/a.md"));
    assert_eq!(search(cx, &links, "newer"), [a]);
    assert_eq!(fs.loads(Path::new("/vault/a.md")) - before, 1);
}

#[gpui::test]
async fn save_fills_the_cache(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "apple")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    saved_edit(cx, &notes, "a", "acorn").await;
    let before = fs.loads(Path::new("/vault/a.md"));

    assert_eq!(search(cx, &links, "acorn"), [a]);
    assert_eq!(fs.loads(Path::new("/vault/a.md")) - before, 0);
}

/// With room for one note, after A's save and, when `close`, its trash: A's loads for a search of
/// A's word, then B's loads for a second search of B's word.
async fn loads_around_a_pin(cx: &mut TestAppContext, close: bool) -> (usize, usize) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "apple"), ("b.md", "berry")]);
    cx.run_until_parked();
    links.update(cx, |links, _| links.set_text_budget(5));
    saved_edit(cx, &notes, "a", "acorn").await;
    if close {
        let a = entry_id(&notes, cx, "a");
        let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
        cx.run_until_parked();
        drop(trashed);
    }
    let (a, b) = (Path::new("/vault/a.md"), Path::new("/vault/b.md"));

    search(cx, &links, "berry");
    let before = fs.loads(a);
    search(cx, &links, "acorn");
    let a_loads = fs.loads(a) - before;
    let before = fs.loads(b);
    search(cx, &links, "berry");
    (a_loads, fs.loads(b) - before)
}

#[gpui::test]
async fn pin_dropped_after_close(cx: &mut TestAppContext) {
    assert_eq!(
        loads_around_a_pin(cx, false).await,
        (0, 1),
        "open, A stays and B makes no room"
    );
    assert_eq!(
        loads_around_a_pin(cx, true).await,
        (0, 0),
        "trashed, A is evicted for B"
    );
}
