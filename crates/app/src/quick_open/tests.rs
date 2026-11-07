use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, New};

use super::QuickOpenDelegate;
use crate::keymap::key_bindings;
use crate::picker::runs::{NOTES_CHANGED_REFRESH, SEARCH_DEBOUNCE};
use crate::picker::{Picker, PickerDelegate as _};
use crate::test_support::{entry_id, index_sync, scan_notes, unscanned_notes, vault};
use crate::vault::NoteStore;

mod background;
mod excerpts;
mod refresh;
mod rows;

pub(super) struct Opened {
    notes: Entity<NoteStore>,
    pub(super) picker: Entity<Picker<QuickOpenDelegate>>,
    pub(super) visual: VisualTestContext,
}

/// `showing` and `recent` by name; the index has built before the picker opens.
pub(super) fn open(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
    showing: Option<&str>,
    recent: &[&str],
) -> Opened {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    cx.run_until_parked();
    let showing = showing.map(|name| entry_id(&notes, cx, name));
    let recent: Vec<EntryId> = recent
        .iter()
        .map(|name| entry_id(&notes, cx, name))
        .collect();
    let (picker, visual) = cx.add_window_view(move |window, cx| {
        let delegate = QuickOpenDelegate::new(&links, showing, recent, cx);
        Picker::new(delegate, window, cx)
    });
    let mut visual = visual.clone();
    visual.update(|window, cx| window.focus(&picker.focus_handle(cx), cx));
    visual.run_until_parked();
    Opened {
        notes,
        picker,
        visual,
    }
}

fn listed(opened: &mut Opened) -> Vec<String> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .list
            .rows
            .iter()
            .map(|row| row.found.path.to_string())
            .collect()
    })
}

fn empty_text(opened: &mut Opened) -> Option<String> {
    opened.picker.read_with(&opened.visual, |picker, cx| {
        picker.delegate.empty_text(cx).map(String::from)
    })
}

/// The cancel flag the newest scan received.
fn last_scan(opened: &mut Opened) -> Arc<AtomicBool> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        let Some(begun) = picker.delegate.runs.begun().last().cloned() else {
            panic!("a scan began");
        };
        begun
    })
}

pub(super) fn with_texts(files: &[(&str, &str)]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for (file, text) in files {
        fs.insert_file(format!("/vault/{file}"), *text);
    }
    fs
}

/// What the footer says now; it draws only after the picker re-renders.
fn footer(opened: &mut Opened) -> Option<String> {
    opened.picker.read_with(&opened.visual, |picker, cx| {
        picker.delegate.footer(cx).map(|footer| footer.to_string())
    })
}

/// Types `keys` and lets the debounced search answer.
pub(super) fn search(opened: &mut Opened, keys: &str) {
    opened.visual.simulate_keystrokes(keys);
    opened.visual.executor().advance_clock(SEARCH_DEBOUNCE);
    opened.visual.run_until_parked();
}

#[gpui::test]
fn an_empty_query_lists_the_recent_notes_without_the_one_showing(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md", "c.md", "d.md"]);
    let mut opened = open(cx, &fs, Some("a"), &["a", "c", "b"]);
    assert_eq!(listed(&mut opened), ["c", "b"]);
}

#[gpui::test]
fn no_recent_note_says_so(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    assert_eq!(listed(&mut opened), Vec::<String>::new());
    assert_eq!(empty_text(&mut opened), Some("No recent notes".into()));
}

#[gpui::test]
fn a_query_with_no_match_says_so(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    opened.visual.simulate_keystrokes("z");
    assert_eq!(empty_text(&mut opened), Some("No matching notes".into()));
}

#[gpui::test]
fn typing_lists_the_matching_notes_best_first(cx: &mut TestAppContext) {
    let fs = vault(&[
        "projects/plan.md",
        "plan.md",
        "plan/notes.md",
        "other.md",
        "planning.md",
        "plans/a long night.md",
    ]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");
    // Whole before prefix, stem before folder; a tie in the stem falls to the newest.
    assert_eq!(
        listed(&mut opened),
        [
            "plan",
            "projects/plan",
            "plan/notes",
            "planning",
            "plans/a long night"
        ]
    );

    // Four terms, each the start of a word, not four letters in order.
    search(
        &mut opened,
        "backspace backspace backspace backspace p space l space a space n",
    );
    assert_eq!(listed(&mut opened), ["plans/a long night"]);
}

#[gpui::test]
fn an_idle_picker_answers_at_once_and_a_busy_one_waits(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "pluto.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);

    opened.visual.simulate_keystrokes("p");
    assert_eq!(
        listed(&mut opened),
        ["pluto", "plan"],
        "no search ran, so no wait"
    );

    // `l` starts at once; `a` finds it running and waits.
    opened.visual.simulate_keystrokes("l a");
    assert_eq!(listed(&mut opened), ["pluto", "plan"]);
    opened
        .visual
        .executor()
        .advance_clock(SEARCH_DEBOUNCE.saturating_sub(Duration::from_millis(1)));
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["pluto", "plan"]);
    opened
        .visual
        .executor()
        .advance_clock(Duration::from_millis(1));
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["plan"]);
}

#[gpui::test]
fn any_core_count_lists_the_same_notes(cx: &mut TestAppContext) {
    let files: Vec<String> = (0..23)
        .map(|n| format!("dir {}/note {n}.md", n % 4))
        .collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let fs = vault(&files);
    let mut lists = Vec::new();
    for cpus in [1, 3, 7] {
        cx.executor().set_num_cpus(cpus);
        let mut opened = open(cx, &fs, None, &[]);
        search(&mut opened, "n o t e space 1");
        lists.push(listed(&mut opened));
    }
    assert!(!lists[0].is_empty());
    assert_eq!(lists[0], lists[1]);
    assert_eq!(lists[0], lists[2]);
}

#[gpui::test]
fn dropping_the_picker_cancels_its_scan(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let cancel = last_scan(&mut opened);
    assert!(!cancel.load(Ordering::Relaxed), "the scan is live");

    opened.visual.update(|window, _| window.remove_window());
    drop(opened.picker);
    // gpui releases a dropped entity when effects flush, and the window is gone.
    opened.visual.cx.update(|_| {});

    assert!(cancel.load(Ordering::Relaxed));
}

#[gpui::test]
fn trashing_a_listed_note_removes_its_row_after_the_refresh_wait(cx: &mut TestAppContext) {
    let fs = vault(&["plan a.md", "plan b.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    let a = entry_id(&opened.notes, &mut opened.visual, "plan a");

    let trashed = opened
        .notes
        .update(&mut opened.visual, |notes, cx| notes.trash(a, cx));
    opened.visual.run_until_parked();
    drop(trashed);
    assert_eq!(
        listed(&mut opened),
        ["plan b", "plan a"],
        "the refresh waits"
    );

    opened
        .visual
        .executor()
        .advance_clock(NOTES_CHANGED_REFRESH);
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["plan b"]);
}

#[gpui::test]
fn a_refresh_keeps_the_selected_note(cx: &mut TestAppContext) {
    let fs = vault(&["plan b.md", "plan c.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    // The newest first: plan c, then plan b.
    opened.visual.simulate_keystrokes("down");

    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, New::Note, "plan a", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    opened
        .visual
        .executor()
        .advance_clock(NOTES_CHANGED_REFRESH);
    opened.visual.run_until_parked();

    assert_eq!(listed(&mut opened), ["plan a", "plan c", "plan b"]);
    let selected = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected());
    assert_eq!(selected, Some(2), "still plan b");
}

/// Zed's file finder reads the selection when the refreshed matches land, not when the refresh starts.
#[gpui::test]
fn a_move_during_the_refresh_wait_is_kept(cx: &mut TestAppContext) {
    let fs = vault(&["plan b.md", "plan c.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");

    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, New::Note, "plan a", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    // Inside the 200 ms wait: the user moves from plan c, the newest, to plan b.
    opened.visual.simulate_keystrokes("down");
    opened
        .visual
        .executor()
        .advance_clock(NOTES_CHANGED_REFRESH);
    opened.visual.run_until_parked();

    assert_eq!(listed(&mut opened), ["plan a", "plan c", "plan b"]);
    let selected = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected());
    assert_eq!(selected, Some(2), "plan b, where the user moved");
}

#[gpui::test]
fn the_picker_says_scanning_then_lists_the_first_names_without_the_refresh_wait(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = vault(&["a.md", "f/b.md"]);
    // Held: typing runs until parked, which would otherwise finish the scan.
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    let (picker, visual) = cx.add_window_view(move |window, cx| {
        let delegate = QuickOpenDelegate::new(&links, None, Vec::new(), cx);
        Picker::new(delegate, window, cx)
    });
    let mut opened = Opened {
        notes,
        picker,
        visual: visual.clone(),
    };
    opened
        .visual
        .update(|window, cx| window.focus(&opened.picker.focus_handle(cx), cx));

    opened.visual.simulate_keystrokes("b");
    assert_eq!(empty_text(&mut opened), Some("Scanning the vault…".into()));
    assert_eq!(listed(&mut opened), Vec::<String>::new());

    gate.release();
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["f/b"], "without the refresh's wait");
}
