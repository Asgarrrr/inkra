//! `Reader`: the candidates' order kept, at most `READERS` in flight, each failure's read.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use futures::{FutureExt as _, StreamExt as _};
use gpui::{Entity, TestAppContext};
use inkra_index::{Candidate, NoteQuery, Read};
use inkra_vault::mem_fs::MemFs;

use super::IndexSync;
use super::reads::READERS;
use super::tests::indexed_vault;

/// The notes holding `alpha`, as `⌘⇧F` would read them now.
fn candidates(cx: &mut TestAppContext, links: &Entity<IndexSync>) -> Vec<Candidate> {
    links.read_with(cx, |links, _| {
        let query = NoteQuery::new("alpha", None, &[]);
        let candidates = links
            .index()
            .snapshot()
            .candidates(&query, &AtomicBool::new(false));
        candidates.expect("not cancelled")
    })
}

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

#[gpui::test]
fn reads_keep_the_candidates_order(cx: &mut TestAppContext) {
    let (fs, links, paths) = alphas(cx, 3);
    let candidates = candidates(cx, &links);
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
    let candidates = candidates(cx, &links);
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
    let candidates = candidates(cx, &links);
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
