use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use gpui::{AppContext as _, Entity, TestAppContext};
use inkra_index::{NoteQuery, TermHit};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind};

use super::IndexSync;
use super::tests::{create, indexed_vault, saved_edit, status};
use crate::test_support::{entry_id, scan_notes};
use crate::timeline::Timeline;

/// The notes of `ids` holding a word that starts with `term`, with their `TermHit`.
fn looked_up(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
    ids: &[EntryId],
    term: &str,
) -> Vec<(EntryId, TermHit)> {
    links.read_with(cx, |links, _| {
        let found = links
            .index()
            .words()
            .lookup(&NoteQuery::new(term, None, &[]), &AtomicBool::new(false))
            .expect("not cancelled");
        ids.iter()
            .filter_map(|id| Some((*id, found.get(0, *id)?)))
            .collect()
    })
}

#[gpui::test]
async fn a_load_and_a_save_index_their_words(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "Loaded été"), ("b.md", "")]);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    assert_eq!(
        looked_up(cx, &links, &[a, b], "ete"),
        [(
            a,
            TermHit {
                count: 1,
                whole: true
            }
        )]
    );

    saved_edit(cx, &notes, "b", "saved Été été").await;

    assert_eq!(
        looked_up(cx, &links, &[a, b], "ete"),
        [
            (
                a,
                TermHit {
                    count: 1,
                    whole: true
                }
            ),
            (
                b,
                TermHit {
                    count: 2,
                    whole: true
                }
            )
        ]
    );
}

#[gpui::test]
fn a_load_finishing_after_a_trash_indexes_no_words(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "old"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "c");
    assert!(!status(cx, &links).is_ready(), "the load is parked");
    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);
    gate.release();
    cx.run_until_parked();

    assert_eq!(looked_up(cx, &links, &[a], "changed"), []);
    assert_eq!(looked_up(cx, &links, &[a], "old"), []);
}

/// Past the merge policy's fresh minimum, so the startup load merges at least once.
const MERGED_NOTES: usize = 600;

#[gpui::test(iterations = 10)]
fn a_startup_load_past_the_merge_policy_merges_and_answers_as_a_fresh_build(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let files: Vec<(String, String)> = (0..MERGED_NOTES)
        .map(|n| (format!("n{n:03}.md"), format!("common w{n}")))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_fs, notes, links) = indexed_vault(cx, &files);
    cx.run_until_parked();
    let (settled, words) = links.read_with(cx, |links, _| {
        let index = links.index();
        (index.words_settled(), index.words().heap_bytes())
    });
    assert!(settled, "the load's merges and settle applied");
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains(&format!("words {words} bytes")), "{line}");

    let ids: Vec<(EntryId, usize)> = notes.read_with(cx, |notes, cx| {
        let vault = notes.vault().expect("a vault").read(cx);
        vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| {
                let name = entry.path.display_name();
                let n = name[1..].parse().expect("a numbered note");
                (entry.id, n)
            })
            .collect()
    });
    assert_eq!(ids.len(), MERGED_NOTES);
    let all: Vec<EntryId> = ids.iter().map(|(id, _)| *id).collect();
    for term in ["common", "w", "w1", "w42", "w599", "x"] {
        let expected: Vec<(EntryId, TermHit)> = ids
            .iter()
            .filter_map(|&(id, n)| {
                let word = ["common".to_owned(), format!("w{n}")]
                    .into_iter()
                    .find(|word| word.starts_with(term))?;
                let whole = word == term;
                Some((id, TermHit { count: 1, whole }))
            })
            .collect();
        assert_eq!(looked_up(cx, &links, &all, term), expected, "{term}");
    }
}

#[gpui::test]
fn a_startup_load_below_the_merge_policy_settles_before_it_is_marked_indexed(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let (_fs, _notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "beta")]);
    cx.run_until_parked();

    let (settled, words) = links.read_with(cx, |links, _| {
        let index = links.index();
        (index.words_settled(), index.words().heap_bytes())
    });
    assert!(settled, "nothing fresh once the loads end");
    // Marked before the merge applied, the line would hold the fresh notes' bytes instead.
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains(&format!("words {words} bytes")), "{line}");
}

#[gpui::test]
fn a_startup_load_ending_in_a_rebuild_settles_before_it_is_marked_indexed(cx: &mut TestAppContext) {
    // One core: `a` lands, `b`'s load is held and `c` waits behind it.
    cx.executor().set_num_cpus(1);
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let fs = Arc::new(MemFs::new());
    for (name, text) in [("a.md", "alpha"), ("b.md", "beta"), ("c.md", "gamma")] {
        fs.insert_file(format!("/vault/{name}"), text);
    }
    let gate = fs.block_load("/vault/b.md");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    cx.run_until_parked();
    assert!(!status(cx, &links).is_ready());

    let (b, c) = (entry_id(&notes, cx, "b"), entry_id(&notes, cx, "c"));
    for id in [b, c] {
        let trashed = notes.update(cx, |notes, cx| notes.trash(id, cx));
        cx.run_until_parked();
        drop(trashed);
    }
    gate.release();
    cx.run_until_parked();

    let (settled, words) = links.read_with(cx, |links, _| {
        let index = links.index();
        (index.words_settled(), index.words().heap_bytes())
    });
    assert!(settled, "the rebuild's settle applied");
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("(1 notes, corpus "), "{line}");
    assert!(line.contains(&format!("words {words} bytes")), "{line}");
}

#[gpui::test]
async fn a_save_after_the_load_settles_waits_in_fresh_for_the_policy(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "")]);
    cx.run_until_parked();
    assert!(links.read_with(cx, |links, _| links.index().words_settled()));

    saved_edit(cx, &notes, "b", "saved").await;
    cx.run_until_parked();

    let settled = links.read_with(cx, |links, _| links.index().words_settled());
    assert!(!settled, "a save starts no merge below the policy");
    let b = entry_id(&notes, cx, "b");
    assert_eq!(
        looked_up(cx, &links, &[b], "saved"),
        [(
            b,
            TermHit {
                count: 1,
                whole: true
            }
        )]
    );
}

/// `a` and an empty `b` among enough merged notes that rewriting one leaves the dead share
/// below the policy's.
const PAST_THE_DEAD_SHARE: &[(&str, &str)] = &[
    ("a.md", "alpha"),
    ("b.md", ""),
    ("n0.md", "filler"),
    ("n1.md", "filler"),
    ("n2.md", "filler"),
    ("n3.md", "filler"),
    ("n4.md", "filler"),
    ("n5.md", "filler"),
    ("n6.md", "filler"),
    ("n7.md", "filler"),
    ("n8.md", "filler"),
    ("n9.md", "filler"),
];

#[gpui::test]
fn a_reload_after_indexed_starts_no_settle(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, PAST_THE_DEAD_SHARE);
    cx.run_until_parked();
    assert!(links.read_with(cx, |links, _| links.index().words_settled()));

    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "new");

    let settled = links.read_with(cx, |links, _| links.index().words_settled());
    assert!(!settled, "a reload waits for the policy");
    let a = entry_id(&notes, cx, "a");
    assert_eq!(
        looked_up(cx, &links, &[a], "changed"),
        [(
            a,
            TermHit {
                count: 1,
                whole: true
            }
        )]
    );
}

#[gpui::test]
async fn autosaves_that_empty_and_refill_a_note_keep_its_ordinal(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "")]);
    cx.run_until_parked();
    let b = entry_id(&notes, cx, "b");
    let span = |cx: &mut TestAppContext| {
        links.read_with(cx, |links, _| links.index().words().ordinal_span())
    };
    saved_edit(cx, &notes, "b", "typed").await;
    let typed = span(cx);
    for text in ["", "again", "", "refilled"] {
        saved_edit(cx, &notes, "b", text).await;
        assert_eq!(span(cx), typed, "after saving {text:?}");
    }
    assert_eq!(looked_up(cx, &links, &[b], "again"), []);
    assert_eq!(
        looked_up(cx, &links, &[b], "refilled"),
        [(
            b,
            TermHit {
                count: 1,
                whole: true
            }
        )]
    );
}
