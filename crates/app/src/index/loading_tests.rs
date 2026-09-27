//! The startup load pipeline: slots per core, one load per note, timed apply turns.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use gpui::{AppContext as _, Entity, TestAppContext};
use inkra_index::NoteContent;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Mtime};

use super::IndexSync;
use super::loading::Landed;
use super::tests::{backlinks, create, found, indexed_vault};
use crate::test_support::{entry_id, scan_notes};
use crate::vault::NoteStore;

#[gpui::test]
fn a_note_queued_twice_is_loaded_once(cx: &mut TestAppContext) {
    // One core, so `a`'s parked load holds the queue and `b`'s first entry is superseded unread.
    cx.executor().set_num_cpus(1);
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let b = Path::new("/vault/b.md");
    assert_eq!(fs.loads(b), 1);

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/b.md", "[[a]]");
    create(cx, &notes, "d");
    fs.insert_file("/vault/b.md", "[[c]]");
    create(cx, &notes, "e");
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.loads(b), 2, "the first queue entry for b was superseded");
    assert_eq!(backlinks(cx, &notes, &links, "c"), ["b"]);
}

#[gpui::test]
fn at_most_one_load_per_core_runs_at_once(cx: &mut TestAppContext) {
    cx.executor().set_num_cpus(2);
    let fs = Arc::new(MemFs::new());
    let paths: Vec<String> = (0..5).map(|n| format!("/vault/n{n}.md")).collect();
    let mut gates = Vec::new();
    for path in &paths {
        fs.insert_file(path.as_str(), "");
        gates.push(fs.block_load(path.as_str()));
    }
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    let started =
        |fs: &MemFs| -> usize { paths.iter().map(|path| fs.loads(Path::new(path))).sum() };
    let progress = |cx: &mut TestAppContext| links.read_with(cx, |links, _| links.progress());

    cx.run_until_parked();
    assert_eq!(started(&fs), 2, "two cores, two loads");
    assert_eq!(progress(cx), (0, 5));

    // The queue follows vault order: n0 and n1 hold the slots.
    gates.remove(0).release();
    cx.run_until_parked();
    assert_eq!(started(&fs), 3, "n0's slot went to n2");
    assert_eq!(fs.loads(Path::new("/vault/n2.md")), 1);
    assert_eq!(progress(cx), (1, 5));

    gates.clear();
    cx.run_until_parked();
    assert_eq!(started(&fs), 5, "each note loaded once");
    assert_eq!(progress(cx), (5, 5));
}

#[gpui::test]
fn a_turn_over_its_budget_yields_and_the_rest_applies_later(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> = (0..6)
        .map(|n| (format!("n{n}.md"), format!("[[n{n}]]")))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let (_fs, _notes, links) = indexed_vault(cx, &files);
    links.update(cx, |links, _| links.apply_budget = Some(Duration::ZERO));

    cx.run_until_parked();

    assert_eq!(
        links.read_with(cx, |links, _| links.turns),
        6,
        "one load per turn"
    );
    assert_eq!(links.read_with(cx, |links, _| links.progress()), (6, 6));
    assert!(!links.read_with(cx, |links, _| links.is_building()));
}

/// All landed before `drain` polls; returns the turns so far.
fn land_at_once(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    links: &Entity<IndexSync>,
    names: &[&str],
    text: &str,
    budget: Duration,
) -> usize {
    let ids: Vec<EntryId> = names.iter().map(|name| entry_id(notes, cx, name)).collect();
    links.update(cx, |links, _| {
        links.apply_budget = Some(budget);
        for id in ids {
            let seq = links.next_seq(id);
            let loaded = Ok(NoteContent::parse(
                Arc::from(text),
                Mtime(SystemTime::UNIX_EPOCH),
            ));
            let built = links.built;
            links
                .landed
                .unbounded_send(Landed {
                    id,
                    seq,
                    built,
                    loaded,
                })
                .expect("drain listens");
        }
        links.turns
    })
}

#[gpui::test]
fn drain_yields_to_other_tasks_between_turns(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let before = land_at_once(
        cx,
        &notes,
        &links,
        &["a", "b", "c"],
        "landed",
        Duration::ZERO,
    );

    // Queued behind `drain`, woken by the sends: it runs between drain's first and second turn.
    let seen = Rc::new(Cell::new(0));
    let sink = Rc::clone(&seen);
    let observed = links.clone();
    cx.spawn(async move |cx| sink.set(cx.update(|cx| observed.read(cx).turns)))
        .detach();
    cx.run_until_parked();

    assert_eq!(
        seen.get(),
        before + 1,
        "a task queued meanwhile ran after one turn"
    );
    assert_eq!(links.read_with(cx, |links, _| links.turns), before + 3);
    assert_eq!(found(cx, &links, "landed").len(), 3);
}

#[gpui::test]
fn a_turn_applies_every_landed_load_within_its_budget(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let before = land_at_once(
        cx,
        &notes,
        &links,
        &["a", "b", "c"],
        "landed",
        Duration::MAX,
    );

    cx.run_until_parked();

    assert_eq!(
        links.read_with(cx, |links, _| links.turns),
        before + 1,
        "one turn applies all three"
    );
    assert_eq!(found(cx, &links, "landed").len(), 3);
}

#[gpui::test]
fn a_note_queued_while_it_loads_waits_for_its_load(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    let progress = |cx: &mut TestAppContext| links.read_with(cx, |links, _| links.progress());
    let a = Path::new("/vault/a.md");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[c]]");
    create(cx, &notes, "c");
    assert_eq!(progress(cx), (2, 3), "a is loading, c is loaded");
    let reads = fs.loads(a);

    // Free slots remain, yet a's new entry waits: one read per note at a time.
    fs.insert_file("/vault/a.md", "[[b]]");
    create(cx, &notes, "d");
    assert_eq!(
        fs.loads(a),
        reads,
        "no second read of a while one is in flight"
    );
    assert_eq!(
        progress(cx),
        (3, 4),
        "a is loading and queued, and counts once"
    );

    gate.release();
    cx.run_until_parked();
    assert_eq!(
        fs.loads(a),
        reads + 1,
        "the queued entry loads once the first lands"
    );
    assert_eq!(progress(cx), (4, 4));
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a"]);
}

#[gpui::test]
fn a_note_renamed_while_it_loads_is_loaded_at_its_new_path(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]] old"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[b]] new");
    create(cx, &notes, "c");
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "z", cx));
    cx.run_until_parked();
    drop(renamed);
    gate.release();
    cx.run_until_parked();

    assert_eq!(found(cx, &links, "new"), [(a, "[[b]] new".to_owned())]);
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["z"]);
    assert_eq!(links.read_with(cx, |links, _| links.progress()), (3, 3));
}

/// Renamed at the `Fs` only: `drain` wakes at the read, before the rename back's first foreground turn.
#[gpui::test]
fn a_load_failing_after_a_build_is_retried_at_the_same_path(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]] old"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let (at_a, at_z) = (Path::new("/vault/a.md"), Path::new("/vault/z.md"));

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[b]] new");
    create(cx, &notes, "c");
    // Stands in for the renames' builds, which queue nothing: a rename keeps the mtime.
    create(cx, &notes, "d");
    futures::executor::block_on(fs.rename(at_a, at_z)).expect("rename away");
    gate.release();
    while cx.dispatcher.tick(true) {}
    futures::executor::block_on(fs.rename(at_z, at_a)).expect("rename back");
    cx.run_until_parked();

    assert_eq!(found(cx, &links, "new"), [(a, "[[b]] new".to_owned())]);
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a"]);
    assert_eq!(links.read_with(cx, |links, _| links.progress()), (4, 4));
}

#[gpui::test]
fn a_load_missing_a_renamed_note_behind_a_save_is_not_retried(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "old"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "new");
    create(cx, &notes, "c");
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "z", cx));
    cx.run_until_parked();
    drop(renamed);
    // What `saved` does before its parse lands: the save, not a reload, brings the text.
    links.update(cx, |sync, _| {
        sync.next_seq(a);
    });
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new("/vault/z.md")), 0);
    assert_eq!(found(cx, &links, "old"), [(a, "old".to_owned())]);
}
