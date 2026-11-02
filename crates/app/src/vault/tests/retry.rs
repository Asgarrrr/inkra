use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui::{Entity, Subscription, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::test_support::{conflicts, edit, opened, two_notes};
use crate::vault::save::{RETRY_LIMIT, retry_delay};
use crate::vault::store::AUTOSAVE_DELAY;
use crate::vault::{NoteStore, SaveFailed};

const A: &str = "/vault/a.md";

fn a_text(fs: &MemFs) -> Option<String> {
    fs.read_to_string(Path::new(A))
}

fn save_failed(notes: &Entity<NoteStore>, cx: &mut TestAppContext, id: EntryId) -> bool {
    notes.read_with(cx, |notes, _| notes.save_failed(id))
}

fn advance(cx: &mut TestAppContext, by: Duration) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
}

fn failures(
    notes: &Entity<NoteStore>,
    cx: &mut TestAppContext,
) -> (Arc<AtomicUsize>, Subscription) {
    let count = Arc::new(AtomicUsize::new(0));
    let subscription = cx.update(|cx| {
        let count = Arc::clone(&count);
        cx.subscribe(notes, move |_, _: &SaveFailed, _| {
            count.fetch_add(1, Ordering::SeqCst);
        })
    });
    (count, subscription)
}

#[gpui::test]
async fn a_transient_failure_retries_after_one_then_two_seconds(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (notes, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&notes, cx, a).await;
    fs.fail_next_saves(A, 2, ErrorKind::Interrupted);

    edit(&buffer, cx, "X");
    advance(cx, AUTOSAVE_DELAY);
    assert_eq!(
        a_text(&fs).as_deref(),
        Some("alpha"),
        "the first write failed"
    );
    assert!(save_failed(&notes, cx, a));

    advance(cx, Duration::from_secs(1));
    assert_eq!(
        a_text(&fs).as_deref(),
        Some("alpha"),
        "the retry after one second failed"
    );

    advance(cx, Duration::from_secs(1));
    assert_eq!(
        a_text(&fs).as_deref(),
        Some("alpha"),
        "the next retry waits two seconds"
    );

    advance(cx, Duration::from_secs(1));
    assert_eq!(a_text(&fs).as_deref(), Some("Xalpha"));
    assert!(
        !save_failed(&notes, cx, a),
        "a landed write clears the failure"
    );
}

#[gpui::test]
async fn a_permanent_failure_waits_for_the_next_edit(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (notes, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&notes, cx, a).await;
    fs.fail_next_saves(A, 1, ErrorKind::PermissionDenied);

    edit(&buffer, cx, "X");
    advance(cx, AUTOSAVE_DELAY);
    advance(cx, Duration::from_mins(1));
    assert_eq!(a_text(&fs).as_deref(), Some("alpha"), "nothing retried");
    assert!(save_failed(&notes, cx, a));

    edit(&buffer, cx, "Y");
    advance(cx, AUTOSAVE_DELAY);
    assert_eq!(a_text(&fs).as_deref(), Some("YXalpha"));
    assert!(
        !save_failed(&notes, cx, a),
        "a landed write clears the failure"
    );
}

/// One fault per attempt the limit allows: one more retry would land the edit before the next one.
#[gpui::test]
async fn a_failing_note_stops_retrying_after_the_limit(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (notes, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&notes, cx, a).await;
    let (failed, _failures) = failures(&notes, cx);
    fs.fail_next_saves(A, 1 + RETRY_LIMIT, ErrorKind::Interrupted);

    edit(&buffer, cx, "X");
    advance(cx, AUTOSAVE_DELAY);
    advance(cx, Duration::from_mins(10));
    assert_eq!(failed.load(Ordering::SeqCst), 1 + RETRY_LIMIT as usize);
    assert_eq!(a_text(&fs).as_deref(), Some("alpha"), "the retries stopped");
    assert!(save_failed(&notes, cx, a));

    edit(&buffer, cx, "Y");
    advance(cx, AUTOSAVE_DELAY);
    assert_eq!(
        a_text(&fs).as_deref(),
        Some("YXalpha"),
        "an edit still saves"
    );
    assert_eq!(failed.load(Ordering::SeqCst), 1 + RETRY_LIMIT as usize);
}

#[gpui::test]
async fn a_retry_after_a_preserving_failure_leaves_one_conflict_copy(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (notes, a, _b) = two_notes(cx, &fs);
    let buffer = opened(&notes, cx, a).await;
    let (preserved, _conflicts) = conflicts(&notes, cx);

    edit(&buffer, cx, "X");
    fs.insert_file(A, "theirs");
    fs.fail_next_saves_after_preserve(A, 3, ErrorKind::Interrupted);
    advance(cx, AUTOSAVE_DELAY);
    advance(cx, Duration::from_mins(1));

    assert_eq!(a_text(&fs).as_deref(), Some("Xalpha"));
    let copies: Vec<_> = futures::executor::block_on(fs.read_dir(Path::new("/vault")))
        .expect("lists")
        .into_iter()
        .filter(|entry| entry.path.to_string_lossy().contains("conflict"))
        .collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(
        fs.read_to_string(&copies[0].path).as_deref(),
        Some("theirs")
    );
    assert_eq!(*preserved.borrow(), [copies[0].path.clone()]);
}

#[test]
fn retry_delay_doubles_from_one_second_to_a_thirty_second_cap() {
    let delays: Vec<_> = [1, 2, 3, 5, 6, 40].into_iter().map(retry_delay).collect();
    let seconds = [1, 2, 4, 16, 30, 30].map(Duration::from_secs);
    assert_eq!(delays, seconds);
}
