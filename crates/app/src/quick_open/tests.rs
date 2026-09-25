use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind};

use super::render::row_text;
use super::{NOTES_CHANGED_REFRESH, QuickOpenDelegate, Run, SEARCH_DEBOUNCE};
use crate::index::IndexSync;
use crate::keymap::key_bindings;
use crate::picker::{Picker, PickerDelegate as _};
use crate::test_support::{entry_id, scan_notes};
use crate::theme;
use crate::vault::NoteStore;

struct Opened {
    notes: Entity<NoteStore>,
    picker: Entity<Picker<QuickOpenDelegate>>,
    visual: VisualTestContext,
}

fn vault(files: &[&str]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for file in files {
        fs.insert_file(format!("/vault/{file}"), "");
    }
    fs
}

/// `showing` and `recent` by name; the index has built before the picker opens.
fn open(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
    showing: Option<&str>,
    recent: &[&str],
) -> Opened {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
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
            .matches
            .iter()
            .map(|found| found.path.to_string())
            .collect()
    })
}

fn empty_text(opened: &mut Opened) -> Option<&'static str> {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.empty_text())
}

fn cancel_flag(opened: &mut Opened) -> Arc<AtomicBool> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        Arc::clone(&picker.delegate.cancel)
    })
}

/// Types `keys` and lets the debounced search answer.
fn search(opened: &mut Opened, keys: &str) {
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
    assert_eq!(empty_text(&mut opened), Some("No recent notes"));
}

#[gpui::test]
fn a_query_with_no_match_says_so(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    opened.visual.simulate_keystrokes("z");
    assert_eq!(empty_text(&mut opened), Some("No matching notes"));
}

#[gpui::test]
fn typing_lists_the_matching_notes_best_first(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "projects/plan.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");

    assert_eq!(listed(&mut opened), ["plan", "projects/plan"]);
}

#[gpui::test]
fn an_idle_picker_answers_at_once_and_a_busy_one_waits(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "pluto.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);

    opened.visual.simulate_keystrokes("p");
    assert_eq!(
        listed(&mut opened),
        ["plan", "pluto"],
        "no search ran, so no wait"
    );

    // `l` starts at once; `a` finds it running and waits.
    opened.visual.simulate_keystrokes("l a");
    assert_eq!(listed(&mut opened), ["plan", "pluto"]);
    opened
        .visual
        .executor()
        .advance_clock(SEARCH_DEBOUNCE.saturating_sub(Duration::from_millis(1)));
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["plan", "pluto"]);
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
        search(&mut opened, "n o 1");
        lists.push(listed(&mut opened));
    }
    assert!(!lists[0].is_empty());
    assert_eq!(lists[0], lists[1]);
    assert_eq!(lists[0], lists[2]);
}

#[gpui::test]
fn a_stale_result_is_dropped(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    opened.visual.simulate_keystrokes("p");

    opened.picker.update(&mut opened.visual, |picker, cx| {
        let stale = picker.delegate.search_id - 1;
        QuickOpenDelegate::apply(
            picker,
            stale,
            Vec::new(),
            false,
            Run::Search,
            Instant::now(),
            cx,
        );
    });

    assert_eq!(listed(&mut opened), ["plan"]);
}

#[gpui::test]
fn a_new_query_cancels_the_scan_before_it(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let first = cancel_flag(&mut opened);

    opened.visual.simulate_keystrokes("p");

    let second = cancel_flag(&mut opened);
    assert!(first.load(Ordering::Relaxed));
    assert!(!second.load(Ordering::Relaxed));
}

#[gpui::test]
fn dropping_the_picker_cancels_its_scan(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let cancel = cancel_flag(&mut opened);

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
        ["plan a", "plan b"],
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
    opened.visual.simulate_keystrokes("down");

    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, Kind::Note, "plan a", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    opened
        .visual
        .executor()
        .advance_clock(NOTES_CHANGED_REFRESH);
    opened.visual.run_until_parked();

    assert_eq!(listed(&mut opened), ["plan a", "plan b", "plan c"]);
    let selected = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected());
    assert_eq!(selected, Some(2), "still plan c");
}

#[gpui::test]
fn an_nfd_file_name_is_listed_and_highlighted_on_its_nfc_path(cx: &mut TestAppContext) {
    let fs = vault(&["cafe\u{301}/e\u{301}te\u{301}.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "e t e");

    let texts = opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .matches
            .iter()
            .map(row_text)
            .collect::<Vec<_>>()
    });

    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].stem.as_ref(), "\u{e9}t\u{e9}");
    assert_eq!(texts[0].folder.as_ref(), "caf\u{e9}");
    let lit: Vec<Range<usize>> = texts[0]
        .stem_highlights
        .iter()
        .map(|(range, _)| range.clone())
        .collect();
    assert_eq!(
        lit,
        [0..2, 2..3, 3..5],
        "byte ranges of the NFC chars, not the NFD ones"
    );
}

#[gpui::test]
fn the_picker_names_itself_and_each_row(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "work/b.md"]);
    let opened = open(cx, &fs, None, &["b", "a"]);
    opened.picker.read_with(&opened.visual, |picker, _| {
        let delegate = &picker.delegate;
        assert_eq!(delegate.key_context(), "QuickOpen");
        assert_eq!(delegate.label(), "Quick open");
        assert_eq!(delegate.placeholder(), "Open a note…");
        // A row reads as its NFC path, the text a screen reader announces.
        assert_eq!(delegate.row_label(0).as_ref(), "work/b");
        assert_eq!(delegate.row_label(1).as_ref(), "a");
        assert_eq!(delegate.row_label(2).as_ref(), "");
    });
}

/// Zed's file finder reads the selection when the refreshed matches land, not when the refresh starts.
#[gpui::test]
fn a_move_during_the_refresh_wait_is_kept(cx: &mut TestAppContext) {
    let fs = vault(&["plan b.md", "plan c.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");

    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, Kind::Note, "plan a", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    // Inside the 200 ms wait: the user moves to plan c.
    opened.visual.simulate_keystrokes("down");
    opened
        .visual
        .executor()
        .advance_clock(NOTES_CHANGED_REFRESH);
    opened.visual.run_until_parked();

    assert_eq!(listed(&mut opened), ["plan a", "plan b", "plan c"]);
    let selected = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected());
    assert_eq!(selected, Some(2), "plan c, where the user moved");
}

/// A file name may hold a newline on macOS; `uniform_list` sizes every row from row 0.
#[gpui::test]
fn a_newline_in_a_file_name_keeps_the_row_one_line(cx: &mut TestAppContext) {
    let fs = vault(&["plan\nb.md", "plan c.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened).len(), 2);
    let row = opened.visual.debug_bounds("picker-row-0");

    assert_eq!(
        row.map(|row| row.size.height),
        Some(theme::UI_LINE_HEIGHT),
        "one line"
    );
    assert!(
        opened.visual.debug_bounds("picker-row-1").is_some(),
        "the list is two lines high, so a two-line row 0 hides row 1"
    );
}

#[gpui::test]
fn a_newline_in_a_file_name_is_a_space_in_the_row_label(cx: &mut TestAppContext) {
    let fs = vault(&["work/plan\nb.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "b");

    let label = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.row_label(0));

    assert_eq!(label.as_ref(), "work/plan b");
}

#[gpui::test]
fn a_stem_wider_than_the_row_is_cut_inside_it(cx: &mut TestAppContext) {
    let long = format!("work/{}.md", "a".repeat(400));
    let fs = vault(&[&long]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "a");
    assert_eq!(listed(&mut opened).len(), 1);

    let row = opened.visual.debug_bounds("picker-row-0");
    let stem = opened.visual.debug_bounds("quick-open-stem");

    assert!(
        row.zip(stem)
            .is_some_and(|(row, stem)| stem.right() <= row.right()),
        "the stem {stem:?} ends inside the row {row:?}"
    );
}
